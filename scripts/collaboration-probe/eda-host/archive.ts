/// <reference types="@jlceda/pro-api-types" />
// One-shot import probe. Never repeat after unknown import result.
void (async () => {
  const ALLOW_IMPORT: boolean = false;
  const fixture = 'acf0e4b3437356162b1742564a56cd8942fdca179333f9958e04a76cf63d83c0';
  const steps: Array<{method:string; value?:unknown}> = [];
  const result: Record<string,unknown> = {probe:'cf584-archive-v1', collectedAt:new Date().toISOString(), steps};
  try {
    const project = await eda.dmt_Project.getCurrentProjectInfo();
    if (project?.uuid !== fixture || !ALLOW_IMPORT) throw new Error('Fixture or one-shot gate mismatch');
    const file = await eda.sys_FileManager.getProjectFile('CF584-Content-20261030');
    if (!file) throw new Error('Export missing');
    steps.push({method:'archive',value:{name:file.name,size:file.size}});
    await eda.sys_FileSystem.saveFile(file,file.name);
    const imported = await eda.sys_FileManager.importProjectByProjectFile(file,'JLCEDA Pro',undefined,
      {operation:'Offline Client Local Path',folderPath:'C:/Users/Ferren/Projects/circuit-fabric/artifacts/collaboration-584-ready/imported'});
    steps.push({method:'import',value:imported ?? null});
    result.currentAfterImport = await eda.dmt_Project.getCurrentProjectInfo();
    result.returnedProject = imported ?? null;
  } catch(error) {result.failure=String(error);console.error('[CF584Archive]',error);}
  finally {await eda.sys_FileSystem.saveFile(new Blob([JSON.stringify(result,null,2)],{type:'application/json'}),'cf584-archive-result.json');}
})();
