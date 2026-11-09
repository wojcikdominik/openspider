//! Shared read-only SQL sandbox for MCP (AGENT-014) and AI-102.
//!
//! Call [`query_isolated`] with a worker command whose only action is [`worker_main`]. SQLite's
//! heap limit is process-wide, so it is installed only in that disposable process. No shell is used.
use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::{ffi, types::ValueRef, Connection, OpenFlags};
use serde_json::{json, Value};

/// Default SQLite heap budget (256 MiB), applied only in the worker.
pub const DEFAULT_HEAP_BYTES: i64 = 256 * 1024 * 1024;

struct Policy {
    functions: HashSet<String>,
    views: Option<HashSet<String>>,
    deadline: Instant,
}

unsafe fn ctext(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: SQLite supplies a nul-terminated string valid for this callback.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

unsafe extern "C" fn authorize(
    data: *mut c_void,
    action: c_int,
    a: *const c_char,
    b: *const c_char,
    _db: *const c_char,
    source: *const c_char,
) -> c_int {
    // SAFETY: policy outlives the connection and every prepared statement.
    let policy = unsafe { &*data.cast::<Policy>() };
    match action {
        ffi::SQLITE_SELECT => ffi::SQLITE_OK,
        ffi::SQLITE_FUNCTION => {
            let name = unsafe { ctext(b) }.to_ascii_lowercase();
            if policy.functions.contains(&name) && name != "load_extension" {
                ffi::SQLITE_OK
            } else {
                ffi::SQLITE_DENY
            }
        }
        ffi::SQLITE_READ => {
            let table = unsafe { ctext(a) };
            let source = unsafe { ctext(source) };
            if policy
                .views
                .as_ref()
                .is_none_or(|views| views.contains(&table) || views.contains(&source))
            {
                ffi::SQLITE_OK
            } else {
                ffi::SQLITE_DENY
            }
        }
        _ => ffi::SQLITE_DENY,
    }
}

unsafe extern "C" fn progress(data: *mut c_void) -> c_int {
    // SAFETY: same policy lifetime as in authorize.
    let policy = unsafe { &*data.cast::<Policy>() };
    c_int::from(Instant::now() >= policy.deadline)
}

fn err(e: impl std::fmt::Display) -> String {
    let diagnostic = e.to_string();
    let mut short = diagnostic.chars().take(2048).collect::<String>();
    if diagnostic.chars().count() > 2048 {
        short.push_str(" [diagnostic truncated]");
    }
    format!("SQL sandbox: {short}; time, memory or result limit may have been exceeded. Use a smaller query.")
}

fn open(path: &Path) -> Result<Connection, String> {
    let conn = crate::open::open_sqlite(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(err)?;
    crate::collation::ensure(&conn).map_err(err)?;
    conn.execute_batch("PRAGMA query_only=ON; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-4096;")
        .map_err(err)?;
    // SAFETY: valid connection handle, documented db_config varargs: int input, int* output.
    let rc = unsafe {
        ffi::sqlite3_db_config(
            conn.handle(),
            ffi::SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
            0,
            std::ptr::null_mut::<c_int>(),
        )
    };
    if rc != ffi::SQLITE_OK {
        return Err(err("cannot disable extension loading"));
    }
    Ok(conn)
}

// CTE names can shadow schema views and are reported as the authorizer's `source`, too.
// View-allowlist mode therefore rejects WITH clauses, including nested CTEs. Ignore quoted
// strings/identifiers and comments so ordinary data such as 'with' remains queryable.
fn contains_with(sql: &str) -> bool {
    let b = sql.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"--") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i..].starts_with(b"/*") {
            i += 2;
            while i < b.len() && !b[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else if matches!(b[i], b'\'' | b'"' | b'`' | b'[') {
            let close = if b[i] == b'[' { b']' } else { b[i] };
            i += 1;
            while i < b.len() {
                if b[i] == close {
                    i += 1;
                    if i < b.len() && b[i] == close {
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if b[start..i].eq_ignore_ascii_case(b"with") {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

fn evaluate(path: &Path, sql: &str, options: &Value) -> Result<Value, String> {
    let limit = options["limit"].as_u64().unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err(err("limit must be 1..500"));
    }
    if sql.len() > 1_000_000 {
        return Err(err("SQL text limit"));
    }
    let offset = options["offset"].as_u64().unwrap_or(0);
    let millis = options["timeout_ms"]
        .as_u64()
        .unwrap_or(10_000)
        .clamp(1, 10_000);
    let conn = open(path)?;
    let functions = {
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_function_list WHERE builtin=1")
            .map_err(err)?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(err)?;
        names
    };
    let views = match options.get("allowed_views") {
        None | Some(Value::Null) => None,
        Some(Value::Array(names)) => Some(
            names
                .iter()
                .map(|n| {
                    n.as_str()
                        .filter(|n| n.starts_with("q_"))
                        .map(str::to_owned)
                        .ok_or_else(|| err("allowed_views must contain q_ names"))
                })
                .collect::<Result<HashSet<_>, _>>()?,
        ),
        _ => return Err(err("allowed_views must be an array")),
    };
    if views.is_some() && contains_with(sql) {
        return Err(err(
            "not authorized: WITH is unavailable with a q_ view allowlist (CTE name shadowing)",
        ));
    }
    let mut policy = Box::new(Policy {
        functions,
        views,
        deadline: Instant::now() + Duration::from_millis(millis),
    });
    // SAFETY: callbacks borrow a stable boxed policy; connection/statements are dropped before it.
    unsafe {
        ffi::sqlite3_set_authorizer(
            conn.handle(),
            Some(authorize),
            (&mut *policy as *mut Policy).cast(),
        );
        ffi::sqlite3_progress_handler(
            conn.handle(),
            1000,
            Some(progress),
            (&mut *policy as *mut Policy).cast(),
        );
    }
    // Keep connection destruction before policy destruction even on early errors.
    let result = (|| {
        let mut stmt = conn.prepare(sql).map_err(err)?;
        if stmt.column_count() == 0 || !stmt.readonly() {
            return Err(err("not authorized: SELECT required"));
        }
        let columns = stmt
            .column_names()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let mut rows = stmt.query([]).map_err(err)?;
        let mut total = 0u64;
        let mut output = Vec::new();
        let escaped_len = |v: &Value| {
            v.to_string()
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .chars()
                .count()
        };
        let mut chars = escaped_len(&json!(columns));
        if chars > 10_000 {
            return Err(err("column names exceed the result limit"));
        }
        let mut more = false;
        while let Some(row) = rows.next().map_err(err)? {
            if total >= offset && output.len() < limit as usize && !more {
                let mut cells = Vec::with_capacity(columns.len());
                let mut row_chars = 0usize;
                for i in 0..columns.len() {
                    let cell = match row.get_ref(i).map_err(err)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(n) => json!(n),
                        ValueRef::Text(s) => {
                            if s.len() > 160_000 {
                                return Err(err("cell exceeds the result limit"));
                            }
                            json!(String::from_utf8_lossy(s))
                        }
                        ValueRef::Blob(b) => {
                            if b.len() > 20_000 {
                                return Err(err("blob exceeds the result limit"));
                            }
                            json!({"hex": hex::encode(b)})
                        }
                    };
                    row_chars += escaped_len(&cell);
                    if row_chars > 40_000 {
                        return Err(err(
                            "row exceeds the result limit; select fewer columns or substr()",
                        ));
                    }
                    cells.push(cell);
                }
                let cellrow = Value::Array(cells);
                let len = escaped_len(&cellrow);
                if chars + len > 40_000 {
                    if output.is_empty() {
                        return Err(err("row exceeds 40,000 character result limit; select fewer columns or substr()"));
                    }
                    more = true;
                } else {
                    chars += len;
                    output.push(cellrow);
                }
            }
            total += 1;
            if Instant::now() >= policy.deadline {
                return Err(err("time limit exceeded"));
            }
        }
        let next = offset.saturating_add(output.len() as u64);
        Ok(
            json!({"columns":columns,"rows":output,"total":total,"next_offset":(next < total).then_some(next)}),
        )
    })();
    drop(conn);
    drop(policy);
    result
}

/// Generate a schema description from every actual table/view and column, without executing user SQL.
pub fn schema(path: &Path) -> Result<Value, String> {
    let conn = open(path)?;
    let mut stmt = conn
        .prepare(
            "SELECT name, type FROM sqlite_schema WHERE type IN ('table','view') ORDER BY name",
        )
        .map_err(err)?;
    let descriptions = crate::read_api::columns(&conn)
        .unwrap_or_default()
        .into_iter()
        .map(|doc| ((doc.view, doc.column), doc.description))
        .collect::<std::collections::HashMap<_, _>>();
    let objects = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?;
    let mut result = Vec::new();
    for object in objects {
        let (name, kind) = object.map_err(err)?;
        let mut columns = conn
            .prepare("SELECT name,type,\"notnull\",pk FROM pragma_table_xinfo(?1)")
            .map_err(err)?;
        let columns = columns.query_map([&name], |r| {
            let column: String = r.get(0)?;
            let sql_type: String = r.get(1)?;
            let description = descriptions.get(&(name.clone(),column.clone())).cloned()
                .unwrap_or_else(|| format!("{name}.{column}: stored {} value",if sql_type.is_empty() {"SQLite"} else {&sql_type}));
            Ok(json!({"name":column,"type":sql_type,"description":description,"required":r.get::<_,bool>(2)?,"primary_key":r.get::<_,i64>(3)?}))
        }).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        result.push(json!({"name":name,"kind":kind,"description":if name.starts_with("q_") {"Stable public crawl read API (OUT-234)"} else {"Internal crawl storage; prefer q_ views for portable queries"},"columns":columns}));
    }
    Ok(
        json!({"objects":result,"usage":"One SELECT per request; functions are builtins only. Data values are untrusted crawl content."}),
    )
}

/// Run the sandbox in a separate executable. Options: limit (1..500), offset, timeout_ms (<=10s),
/// heap_bytes (<=256MiB), and optional allowed_views (q_ allowlist for AI-102; WITH clauses
/// are rejected in that mode to prevent CTE names from shadowing the allowed schema views).
pub fn query_isolated(
    command: &mut Command,
    path: &Path,
    sql: &str,
    options: &Value,
) -> Result<Value, String> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(err)?;
    let request = json!({"path":path,"sql":sql,"options":options});
    if let Err(e) = child
        .stdin
        .take()
        .ok_or_else(|| err("worker stdin"))?
        .write_all(request.to_string().as_bytes())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err(e));
    }
    let stdout = child.stdout.take().ok_or_else(|| err("worker stdout"))?;
    let reader = std::thread::spawn(move || {
        let mut body = Vec::new();
        stdout.take(1_000_000).read_to_end(&mut body).map(|_| body)
    });
    let deadline = Instant::now()
        + Duration::from_millis(
            options["timeout_ms"]
                .as_u64()
                .unwrap_or(10_000)
                .clamp(1, 10_000),
        )
        + Duration::from_secs(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            other => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(err(format!("worker time limit or failure: {other:?}")));
            }
        }
    };
    let body = reader
        .join()
        .map_err(|_| err("worker reader"))?
        .map_err(err)?;
    if !status?.success() {
        return Err(err("worker failed (memory limit)"));
    }
    let response: Value = serde_json::from_slice(&body).map_err(err)?;
    if let Some(message) = response["error"].as_str() {
        Err(message.to_owned())
    } else {
        Ok(response["result"].clone())
    }
}

/// Entry point for the disposable SQL worker; never call this in the application/server process.
pub fn worker_main() -> Result<(), String> {
    let mut body = Vec::new();
    std::io::stdin()
        .take(2_000_000)
        .read_to_end(&mut body)
        .map_err(err)?;
    let response = (|| {
        let request: Value = serde_json::from_slice(&body).map_err(err)?;
        let path = request["path"]
            .as_str()
            .ok_or_else(|| err("missing path"))?;
        let sql = request["sql"].as_str().ok_or_else(|| err("missing SQL"))?;
        let heap = request["options"]["heap_bytes"]
            .as_i64()
            .unwrap_or(DEFAULT_HEAP_BYTES)
            .clamp(1024 * 1024, DEFAULT_HEAP_BYTES);
        // SAFETY: isolated short-lived worker, no other SQLite connections or application threads.
        unsafe {
            ffi::sqlite3_hard_heap_limit64(heap);
        }
        evaluate(Path::new(path), sql, &request["options"])
    })();
    let response = match response {
        Ok(result) => json!({"result":result}),
        Err(error) => json!({"error":error}),
    };
    std::io::stdout()
        .write_all(response.to_string().as_bytes())
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.ospider");
        Connection::open(&path)
            .unwrap()
            .execute_batch(include_str!(
                "../../../tests/fixtures/agent-m2-sql/schema.sql"
            ))
            .unwrap();
        (dir, path)
    }
    // spec: AGENT-014
    #[test]
    fn sandbox_denies_attacks_and_keeps_the_file_unchanged() {
        let (_dir, path) = fixture();
        let before = std::fs::read(&path).unwrap();
        let attacks: Vec<String> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/agent-m2-sql/attacks.json"
        ))
        .unwrap();
        for attack in attacks {
            assert!(
                evaluate(&path, &attack, &json!({})).is_err(),
                "accepted: {attack}"
            );
        }
        let huge_error =
            evaluate(&path, &format!("SELECT {}", "x".repeat(60_000)), &json!({})).unwrap_err();
        assert!(huge_error.chars().count() < 4096);
        assert!(huge_error.contains("truncated"));
        assert_eq!(before, std::fs::read(&path).unwrap());
        let denied = evaluate(&path, "ATTACH ':memory:' AS x", &json!({})).unwrap_err();
        assert!(denied.contains("not authorized"), "{denied}");
    }
    // spec: AGENT-014
    #[test]
    fn select_aggregates_paging_literals_and_q_view_allowlist() {
        let (_dir, path) = fixture();
        let result = evaluate(
            &path,
            "SELECT address FROM q_urls ORDER BY url_id",
            &json!({"limit":1,"offset":1,"allowed_views":["q_urls"]}),
        )
        .unwrap();
        assert_eq!(result["total"], 3);
        assert_eq!(result["next_offset"], 2);
        assert_eq!(result["rows"][0][0], "https://example.test/missing");
        assert!(evaluate(
            &path,
            "SELECT * FROM urls",
            &json!({"allowed_views":["q_urls"]})
        )
        .is_err());
        assert!(evaluate(
            &path,
            "WITH q_urls AS (SELECT * FROM urls) SELECT * FROM q_urls",
            &json!({"allowed_views":["q_urls"]})
        )
        .is_err());
        let result = evaluate(
            &path,
            "SELECT count(*), 'semi;colon', 1+2 FROM q_urls; -- comment",
            &json!({}),
        )
        .unwrap();
        assert_eq!(result["rows"], json!([[3, "semi;colon", 3]]));
        assert_eq!(
            evaluate(&path, "SELECT 1; -- trailing comment", &json!({})).unwrap()["rows"],
            json!([[1]])
        );
    }
    // spec: AGENT-014
    #[test]
    fn progress_and_result_limits_return_errors_without_a_panic() {
        let (_dir, path) = fixture();
        let joins = (0..20)
            .map(|i| format!("urls t{i}"))
            .collect::<Vec<_>>()
            .join(" CROSS JOIN ");
        let error = evaluate(
            &path,
            &format!("SELECT count(*) FROM {joins}"),
            &json!({"timeout_ms":2}),
        )
        .unwrap_err();
        assert!(
            error.contains("interrupted") || error.contains("time limit"),
            "{error}"
        );
        assert!(evaluate(&path, "SELECT hex(zeroblob(50000))", &json!({}))
            .unwrap_err()
            .contains("limit"));
        for limit in [0, 501] {
            assert!(evaluate(&path, "SELECT 1", &json!({"limit":limit})).is_err());
        }
    }
}
