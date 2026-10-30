/// <reference types="@jlceda/pro-api-types" />
// Run once in the fixture; may show a native unsaved confirmation dialog.
void (async () => {
  const ALLOW: boolean = false;
  const fixture='acf0e4b3437356162b1742564a56cd8942fdca179333f9958e04a76cf63d83c0';
  const page='0b6e010013c4d426';
  const marker='CF584-UNSAVED-20261030-1220';
  const result: Record<string,unknown>={probe:'cf584-unsaved-close-v1',collectedAt:new Date().toISOString(),marker};
  try {
    if (!ALLOW || (await eda.dmt_Project.getCurrentProjectInfo())?.uuid !== fixture) throw new Error('Gate/fixture mismatch');
    const tab=await eda.dmt_EditorControl.openDocument(page);
    if (!tab || (await eda.dmt_Schematic.getCurrentSchematicPageInfo())?.uuid !== page) throw new Error('Page mismatch');
    result.before=await eda.sch_PrimitiveText.getAll();
    if (JSON.stringify(result.before).includes(marker)) throw new Error('Already ran; do not repeat');
    result.created=await eda.sch_PrimitiveText.create(220,180,marker);
    result.beforeClose=await eda.sch_PrimitiveText.getAll();
    result.closeReturned=await eda.dmt_EditorControl.closeDocument(tab);
    result.reopenedTab=await eda.dmt_EditorControl.openDocument(page);
    result.after=await eda.sch_PrimitiveText.getAll();
    result.markerAfterReopen=JSON.stringify(result.after).includes(marker);
  } catch(error) {result.failure=String(error);console.error('[CF584Dirty]',error);}
  finally {await eda.sys_FileSystem.saveFile(new Blob([JSON.stringify(result,null,2)],{type:'application/json'}),'cf584-dirty-result.json');}
})();
