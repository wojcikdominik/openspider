import { comparisonRows, type ComparisonQuery, useComparison } from "./model";
/** Export the complete active projection, including all comparison columns, through bounded core pages. */
export async function comparisonCsv(query: ComparisonQuery, columns: string[] = []): Promise<string> {
  const session=useComparison.getState().session;const revision=useComparison.getState().revision;
  const lines:string[]=[];let offset=0;
  const field=(value:unknown)=>{const text=String(value??"");return /[",\r\n]/.test(text)?`"${text.replaceAll('"','""')}"`:text;};
  for(;;){
    const page=await comparisonRows(query,offset,undefined,100,columns);
    if(useComparison.getState().session!==session || useComparison.getState().revision!==revision)throw new Error("Comparison changed during export");
    if(offset===0)lines.push(page.columns.map(field).join(","));
    lines.push(...page.cells.map(row=>row.map(field).join(",")));
    offset+=page.cells.length;
    if(offset>=page.total)break;
    if(page.cells.length===0)throw new Error("Incomplete comparison page");
  }
  return lines.join("\r\n")+"\r\n";
}
export async function downloadComparison(query:ComparisonQuery,name:string,columns:string[]=[]):Promise<void>{
  const csv=await comparisonCsv(query,columns);const url=URL.createObjectURL(new Blob([csv],{type:"text/csv;charset=utf-8"}));
  const a=document.createElement("a");a.href=url;a.download=name;document.body.append(a);a.click();a.remove();
  setTimeout(()=>URL.revokeObjectURL(url),0);
}
