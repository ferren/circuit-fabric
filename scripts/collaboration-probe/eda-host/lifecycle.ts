/// <reference types="@jlceda/pro-api-types" />
// Reviewable native-host probe. Change this gate only after the user has saved
// the current project and explicitly allowed switching to the test project.
void (async () => {
  const ALLOW_SWITCH_AFTER_SAVE: boolean = false;
  const expectedOriginal = '135d2e3c9573727175c3c7596ab2f66b90484cf174de38bfb60d0c03aa72d9ea';
  const testName = 'CF584-Lifecycle-20261030';
  const targetFolder = 'C:/Users/Ferren/Projects/circuit-fabric/artifacts/cf584-native-projects';
  const steps: Array<{ method: string; value?: unknown; error?: string }> = [];
  const result: Record<string, unknown> = { schemaVersion: 1, probe: 'cf584-lifecycle-v1',
    collectedAt: new Date().toISOString(), testName, targetFolder, steps,
    sync: 'unverified; no explicit document sync API established',
    deletion: 'disabled', checkpoint: 'unverified',
    activeSdkVersion: null,
    sdkDeclarations: '@jlceda/pro-api-types 0.4.27' };
  async function step<T>(method: string, fn: () => Promise<T>): Promise<T> {
    try {
      const value = await fn();
      steps.push({ method, value });
      return value;
    } catch (error) {
      steps.push({ method, error: String(error) });
      throw error;
    }
  }
  try {
    if (!ALLOW_SWITCH_AFTER_SAVE) throw new Error('User saved-state confirmation required; no writes performed');
    result.runtimeVersion = await step('getEditorCurrentVersion', async () => eda.sys_Environment.getEditorCurrentVersion());
    const original = await step('getCurrentProjectInfo.before', () => eda.dmt_Project.getCurrentProjectInfo());
    if (original?.uuid !== expectedOriginal) throw new Error('Original project changed; no writes performed');
    // No automatic retry: if creation result is unknown, inspect before rerunning.
    const projectId = await step('createProject', () => eda.dmt_Project.createProject(testName, testName, targetFolder));
    if (!projectId) throw new Error('createProject returned no UUID; do not retry blindly');
    result.testProjectUuid = projectId;
    if (!await step('openProject', () => eda.dmt_Project.openProject(projectId))) throw new Error('openProject returned false');
    const current = await step('getCurrentProjectInfo.created', () => eda.dmt_Project.getCurrentProjectInfo());
    if (current?.uuid !== projectId) throw new Error('Current project does not match created UUID; no document writes');
    const schematicId = await step('createSchematic', () => eda.dmt_Schematic.createSchematic());
    if (!schematicId) throw new Error('createSchematic returned no UUID');
    const pages = await step('getAllSchematicPagesInfo.initial', () => eda.dmt_Schematic.getAllSchematicPagesInfo());
    const testPages = pages.filter(p => p.parentSchematicUuid === schematicId).map(p => p.uuid);
    while (testPages.length < 2) {
      const page = await step('createSchematicPage', () => eda.dmt_Schematic.createSchematicPage(schematicId));
      if (!page || testPages.includes(page)) throw new Error('create page missing or duplicate identity');
      testPages.push(page);
    }
    result.testPageUuids = testPages;
    for (const [index, uuid] of testPages.slice(0, 2).entries()) {
      if (!await step('modifySchematicPageName', () => eda.dmt_Schematic.modifySchematicPageName(uuid, index === 0 ? 'Probe-A' : 'Probe-B'))) {
        throw new Error('page rename returned false');
      }
      const tab = await step('openDocument', () => eda.dmt_EditorControl.openDocument(uuid));
      if (!tab) throw new Error('openDocument returned no tab');
      const active = await step('getCurrentSchematicPageInfo', () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
      if (active?.uuid !== uuid) throw new Error('Active page identity mismatch; no save');
      if (!await step('save', () => eda.sch_Document.save())) throw new Error('save returned false');
    }
    result.finalTree = await step('getCurrentProjectInfo.final', () => eda.dmt_Project.getCurrentProjectInfo());
    const first = testPages[0];
    await step('openDocument.reopenA', () => eda.dmt_EditorControl.openDocument(first));
    const reopened = await step('getCurrentSchematicPageInfo.reopenA', () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
    if (reopened?.uuid !== first) throw new Error('Page UUID changed on reopen');
    result.samePageAfterReopen = true;
    const projectFile = await step('getProjectFile', () => eda.sys_FileManager.getProjectFile(testName));
    if (projectFile) await eda.sys_FileSystem.saveFile(projectFile, projectFile.name);
    result.completedSteps = 'create/open/rename/save/reopen; sync and restart remain unverified';
  } catch (error) {
    result.failure = String(error);
    console.error('[CF584Lifecycle]', error);
  } finally {
    // Never delete the fixture or switch away after an unknown write result.
    await eda.sys_FileSystem.saveFile(new Blob([JSON.stringify(result, null, 2)],
      { type: 'application/json' }), 'cf584-lifecycle-result.json');
  }
})().catch(error => eda.sys_Dialog.showInformationMessage(String(error), 'CF584 生命周期探测失败'));
