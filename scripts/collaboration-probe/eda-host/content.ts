/// <reference types="@jlceda/pro-api-types" />
// beta text APIs: verify on the target host. Never run on a user project.
void (async () => {
  const WRITE: boolean = false; // Enable only in a reviewed one-off artifact copy.
  const fixture = 'acf0e4b3437356162b1742564a56cd8942fdca179333f9958e04a76cf63d83c0';
  const pages = ['0b6e010013c4d426', '112ce6c2685aa137'];
  const markers = ['CF584-CONTENT-A-20261030', 'CF584-CONTENT-B-20261030'];
  const steps: Array<{method: string; value?: unknown; error?: string}> = [];
  const result: Record<string, unknown> = {schemaVersion: 1, probe: 'cf584-content-v1',
    collectedAt: new Date().toISOString(), fixture, pages, markers, write: WRITE, steps,
    sdkDeclarations: '@jlceda/pro-api-types 0.4.27', activeSdkVersion: null};
  async function step<T>(method: string, fn: () => Promise<T>): Promise<T> {
    try { const value = await fn(); steps.push({method, value: JSON.parse(JSON.stringify(value ?? null))}); return value; }
    catch (error) { steps.push({method, error: String(error)}); throw error; }
  }
  try {
    result.runtimeVersion = eda.sys_Environment.getEditorCurrentVersion();
    const project = await step('project', () => eda.dmt_Project.getCurrentProjectInfo());
    if (project?.uuid !== fixture) throw new Error('Fixture mismatch; no document writes');
    for (const [i, page] of pages.entries()) {
      const tab = await step(`${i}.open`, () => eda.dmt_EditorControl.openDocument(page));
      if (!tab) throw new Error('Missing tab');
      const active = await step(`${i}.identity`, () => eda.dmt_Schematic.getCurrentSchematicPageInfo());
      if (active?.uuid !== page) throw new Error('Active UUID mismatch');
      const before = await step(`${i}.before`, () => eda.sch_PrimitiveText.getAll());
      const serialized = JSON.stringify(before);
      if (serialized.includes(markers[1-i])) throw new Error('Cross-page content');
      if (!serialized.includes(markers[i])) {
        if (!WRITE) throw new Error('Expected persisted marker missing');
        await step(`${i}.create`, () => eda.sch_PrimitiveText.create(100 + i*60, 100, markers[i]));
      }
      if (WRITE && !await step(`${i}.save`, () => eda.sch_Document.save())) throw new Error('Save false');
      const texts = await step(`${i}.read`, () => eda.sch_PrimitiveText.getAll());
      if (!JSON.stringify(texts).includes(markers[i])) throw new Error('Content not observed');
      if (WRITE) {
        if (!await step(`${i}.close`, () => eda.dmt_EditorControl.closeDocument(tab))) throw new Error('Close false');
        await step(`${i}.reopen`, () => eda.dmt_EditorControl.openDocument(page));
        const reopened = await step(`${i}.reopened`, () => eda.sch_PrimitiveText.getAll());
        if (!JSON.stringify(reopened).includes(markers[i])) throw new Error('Persisted marker missing');
      }
    }
    result.passed = true;
  } catch (error) { result.failure = String(error); console.error('[CF584Content]', error); }
  finally { await eda.sys_FileSystem.saveFile(new Blob([JSON.stringify(result, null, 2)],
    {type:'application/json'}), 'cf584-content.json'); }
})();
