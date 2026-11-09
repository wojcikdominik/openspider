//! Update IPC contracts (M3.md 3.2).
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DisableReason {
    Settings,
    Environment,
    MissingPublicKey,
    InvalidPublicKey,
    NotImplemented,
    // Windows: Installationsart (MSI/NSIS) nicht erkannt; die UI verweist auf den manuellen Download.
    UnknownInstaller,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PendingInstall {
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum InstallWhen {
    Now,
    OnQuit,
    AfterCrawl,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UpdateState {
    Disabled(DisableReason),
    Idle,
    Checking,
    UpToDate,
    Available(AvailableUpdate),
    Downloading { done: u64, total: Option<u64> },
    Ready(PendingInstall),
    Failed(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallArgs {
    pub when: InstallWhen,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSetAutoArgs {
    pub check: bool,
}

/// App-wide settings wire format; follows AppSettings.updates field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsDto {
    pub check: bool,
    pub last_check: Option<String>,
}
