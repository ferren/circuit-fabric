# Build artifacts

`circuitfabric-jlc-eda-extension_v0.2.2.eext` is the distributable JLCircuit EDA extension package included with this baseline.

To rebuild it from the checked-in extension source, run from the repository root:

```powershell
./scripts/package-jlc-extension.ps1
```

The package is generated from `plugins/jlcircuit-eda-extension/`; the source directory remains the canonical implementation.
