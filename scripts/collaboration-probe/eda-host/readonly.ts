/// <reference types="@jlceda/pro-api-types" />
// Run the compiled JS in EDA Advanced > Run Script. Reads only.
// @beta getters are probed explicitly; missing/failing calls remain unverified.
void (async () => {
  const calls: Record<string, unknown> = {};
  async function read(name: string, fn: () => unknown): Promise<void> {
    try { calls[name] = { status: 'observed', value: await fn() }; }
    catch (error) { calls[name] = { status: 'unverified', error: String(error) }; }
  }
  await read('runtimeVersion', () => eda.sys_Environment.getEditorCurrentVersion());
  await read('project', () => eda.dmt_Project.getCurrentProjectInfo());
  await read('schematics', () => eda.dmt_Schematic.getAllSchematicsInfo());
  await read('pages', () => eda.dmt_Schematic.getAllSchematicPagesInfo());
  await read('pageBefore', () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
  await read('editorTree', () => eda.dmt_EditorControl.getSplitScreenTree());
  await read('pageAfter', () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
  const result = { schemaVersion: 1, probe: 'cf584-native-readonly-v1',
    collectedAt: new Date().toISOString(), calls,
    sdkDeclarations: '@jlceda/pro-api-types 0.4.27',
    activeSdkVersion: null, lifecycle: 'unverified', scope: 'native project/tree reads only' };
  const blob = new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' });
  try {
    const saved = await eda.sys_FileSystem.saveFileToFileSystem(
      'C:/Users/Ferren/Projects/circuit-fabric/artifacts/collaboration-584-connected/native-readonly.json', blob);
    if (!saved) throw new Error('native save returned false');
  } catch (error) {
    console.warn('[CF584Probe] direct evidence export unavailable', error);
    await eda.sys_FileSystem.saveFile(blob, 'cf584-native-readonly.json');
  }
})().catch(error => {
  console.error('[CF584Probe]', error);
  eda.sys_Dialog.showInformationMessage(String(error), 'CF584 只读探测失败');
});
