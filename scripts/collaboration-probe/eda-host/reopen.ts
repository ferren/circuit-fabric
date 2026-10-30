/// <reference types="@jlceda/pro-api-types" />
// Run only against the fixture whose native lifecycle result was reviewed.
void (async () => {
  const fixture = 'acf0e4b3437356162b1742564a56cd8942fdca179333f9958e04a76cf63d83c0';
  const original = '135d2e3c9573727175c3c7596ab2f66b90484cf174de38bfb60d0c03aa72d9ea';
  const pages = ['0b6e010013c4d426', '112ce6c2685aa137'];
  const steps: Array<{ method: string; value?: unknown; error?: string }> = [];
  const result: Record<string, unknown> = {
    schemaVersion: 1, probe: 'cf584-reopen-v1', collectedAt: new Date().toISOString(),
    fixture, pages, steps, sdkDeclarations: '@jlceda/pro-api-types 0.4.27', activeSdkVersion: null,
    scope: 'native document close/reopen and project switch away/back in one host process; no restart or cloud sync',
  };
  async function step<T>(method: string, fn: () => Promise<T>): Promise<T> {
    try { const value = await fn(); steps.push({ method, value }); return value; }
    catch (error) { steps.push({ method, error: String(error) }); throw error; }
  }
  async function requireProject(uuid: string, label: string) {
    const current = await step(label, () => eda.dmt_Project.getCurrentProjectInfo());
    if (current?.uuid !== uuid) throw new Error('Project UUID mismatch; stopping');
    return current;
  }
  try {
    result.runtimeVersion = eda.sys_Environment.getEditorCurrentVersion();
    await requireProject(fixture, 'fixture.before');
    for (const [i, uuid] of pages.entries()) {
      const tab = await step(`page${i}.open`, () => eda.dmt_EditorControl.openDocument(uuid));
      if (!tab) throw new Error('Missing tab identity');
      const active = await step(`page${i}.beforeClose`, () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
      if (active?.uuid !== uuid) throw new Error('Page UUID mismatch before save');
      if (!await step(`page${i}.save`, () => eda.sch_Document.save())) throw new Error('Save failed');
      if (!await step(`page${i}.close`, () => eda.dmt_EditorControl.closeDocument(tab))) throw new Error('Close failed');
      await step(`page${i}.editorAfterClose`, () => eda.dmt_EditorControl.getSplitScreenTree());
      if (!await step(`page${i}.reopen`, () => eda.dmt_EditorControl.openDocument(uuid))) throw new Error('Reopen failed');
      const reopened = await step(`page${i}.afterReopen`, () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
      if (reopened?.uuid !== uuid || reopened.name !== active.name) throw new Error('Page identity/name changed after close/reopen');
    }
    if (!await step('switchToOriginal', () => eda.dmt_Project.openProject(original))) throw new Error('Switch to original failed');
    await requireProject(original, 'original.readOnly');
    if (!await step('switchBackToFixture', () => eda.dmt_Project.openProject(fixture))) throw new Error('Switch back failed');
    await requireProject(fixture, 'fixture.afterSwitch');
    const allPages = await step('fixture.pagesAfterSwitch', () => eda.dmt_Schematic.getAllSchematicPagesInfo());
    for (const [i, uuid] of pages.entries()) {
      const info = allPages.find(p => p.uuid === uuid);
      if (!info || info.name !== (i === 0 ? 'Probe-A' : 'Probe-B')) throw new Error('Saved fixture page identity/name missing after project switch');
    }
    result.stableAfterDocumentClose = true;
    result.stableAfterProjectSwitch = true;
    // Return the saved user project to the foreground without modifying it.
    if (!await step('restoreOriginal', () => eda.dmt_Project.openProject(original))) throw new Error('Original restore failed');
    await requireProject(original, 'original.restored');
    await step('restoreOriginalPage', () => eda.dmt_EditorControl.openDocument('5dfc602f34949c83'));
    result.originalRestored = true;
  } catch (error) {
    result.failure = String(error);
    console.error('[CF584Reopen]', error);
  } finally {
    await eda.sys_FileSystem.saveFile(new Blob([JSON.stringify(result, null, 2)],
      { type: 'application/json' }), 'cf584-reopen-result.json');
  }
})().catch(error => eda.sys_Dialog.showInformationMessage(String(error), 'CF584 重开探测失败'));
