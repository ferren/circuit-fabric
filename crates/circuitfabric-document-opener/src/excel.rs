//! Excel opener: cached cell values via `calamine`.
//!
//! Both legacy `.xls` (BIFF) and `.xlsx` are read. Only cached worksheet values are
//! surfaced — formulas are never recalculated, macros are never executed, and external
//! links are never followed. Rows and cell text are capped by the protocol limits.

use std::io::Cursor;

use calamine::{Data, Reader, open_workbook_auto_from_rs};
use circuitfabric_plugin_api::{
    DocumentCell, DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest, DocumentSheet,
    DocumentViewBody, MAX_DOCUMENT_VIEW_ROWS_PER_SHEET, OpenerCapability,
};

use crate::{capped_text, view};

/// Opens Excel managed copies as a sheet-oriented, read-only value view.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExcelOpener;

impl DocumentOpener for ExcelOpener {
    fn capability(&self) -> OpenerCapability {
        OpenerCapability {
            opener_id: "builtin-excel".to_owned(),
            label: "Excel workbook".to_owned(),
            extensions: vec!["xls".to_owned(), "xlsx".to_owned()],
            mime_types: vec![
                "application/vnd.ms-excel".to_owned(),
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_owned(),
            ],
        }
    }

    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let cursor = Cursor::new(request.managed_copy.data().to_vec());
        let mut workbook = match open_workbook_auto_from_rs(cursor) {
            Ok(workbook) => workbook,
            Err(error) => {
                return DocumentOpenerOutcome::Failed {
                    reason: format!("the workbook cannot be opened: {error}"),
                };
            }
        };
        let names = workbook.sheet_names();
        let mut sheets = Vec::with_capacity(names.len());
        for name in &names {
            let range = match workbook.worksheet_range(name) {
                Ok(range) => range,
                Err(error) => {
                    return DocumentOpenerOutcome::Failed {
                        reason: format!("the worksheet `{name}` cannot be read: {error}"),
                    };
                }
            };
            let row_count = range.rows().count();
            let truncated = row_count > MAX_DOCUMENT_VIEW_ROWS_PER_SHEET;
            let rows = range
                .rows()
                .take(MAX_DOCUMENT_VIEW_ROWS_PER_SHEET)
                .map(|row| row.iter().map(cell_value).collect())
                .collect();
            sheets.push(DocumentSheet { name: name.clone(), rows, row_count, truncated });
        }
        if sheets.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the workbook contains no worksheets".to_owned(),
            };
        }
        DocumentOpenerOutcome::Loaded {
            view: view("builtin-excel", request, DocumentViewBody::Sheets { sheets }),
        }
    }
}

/// Maps one cached cell value; error cells surface as a sentinel, never an evaluation.
///
/// Integer cells beyond 2^53 lose precision in the `f64` view, which is acceptable for a
/// read-only preview.
#[allow(clippy::cast_precision_loss)]
fn cell_value(cell: &Data) -> DocumentCell {
    match cell {
        Data::Empty | Data::DateTimeIso(_) | Data::DurationIso(_) => DocumentCell::Empty,
        Data::String(text) => DocumentCell::Text(capped_text(text)),
        Data::Float(value) => DocumentCell::Number(*value),
        Data::Int(value) => DocumentCell::Number(*value as f64),
        Data::Bool(value) => DocumentCell::Boolean(*value),
        Data::DateTime(value) => DocumentCell::Number(value.as_f64()),
        // Formula errors are rendered as a marker; the failing expression is never evaluated.
        Data::Error(_) => DocumentCell::Text("#ERROR".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{DocumentCell, DocumentLoadState, DocumentOpenerOutcome};

    use super::*;
    use crate::testing;

    /// Minimal but complete `.xlsx` package: content types, relationships, workbook, shared
    /// strings, and one worksheet with strings, numbers, and a boolean.
    fn minimal_xlsx() -> Vec<u8> {
        let entries = [
            (
                "[Content_Types].xml",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
                 <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
                 <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
                 <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
                 <Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
                 <Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>\
                 </Types>"
                    .to_owned(),
            ),
            (
                "_rels/.rels",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
                 </Relationships>"
                    .to_owned(),
            ),
            (
                "xl/workbook.xml",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
                 <sheets><sheet name=\"BOM\" sheetId=\"1\" r:id=\"rId1\"/></sheets>\
                 </workbook>"
                    .to_owned(),
            ),
            (
                "xl/_rels/workbook.xml.rels",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>\
                 <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>\
                 </Relationships>"
                    .to_owned(),
            ),
            (
                "xl/sharedStrings.xml",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"3\" uniqueCount=\"3\">\
                 <si><t>Designator</t></si><si><t>R1</t></si><si><t>Value</t></si>\
                 </sst>"
                    .to_owned(),
            ),
            (
                "xl/worksheets/sheet1.xml",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
                 <sheetData>\
                 <row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>2</v></c></row>\
                 <row r=\"2\"><c r=\"A2\" t=\"s\"><v>1</v></c><c r=\"B2\"><v>10000</v></c></row>\
                 </sheetData>\
                 </worksheet>"
                    .to_owned(),
            ),
        ];
        let references: Vec<(&str, String)> =
            entries.iter().map(|(name, content)| (*name, content.clone())).collect();
        testing::zip_bytes(&references)
    }

    #[test]
    fn an_xlsx_loads_its_cached_values_as_sheets() {
        let bytes = minimal_xlsx();
        let request = testing::request_for("bom.xlsx", &bytes);

        let outcome = ExcelOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("a valid workbook must load, got {outcome:?}");
        };
        assert_eq!(view.title, "bom");
        let DocumentViewBody::Sheets { sheets } = view.body else {
            panic!("workbooks render as sheets");
        };
        assert_eq!(sheets.len(), 1);
        let sheet = &sheets[0];
        assert_eq!(sheet.name, "BOM");
        assert_eq!(sheet.row_count, 2);
        assert!(!sheet.truncated);
        assert_eq!(sheet.rows[0][0], DocumentCell::Text("Designator".to_owned()));
        assert_eq!(sheet.rows[0][1], DocumentCell::Text("Value".to_owned()));
        assert_eq!(sheet.rows[1][0], DocumentCell::Text("R1".to_owned()));
        assert_eq!(sheet.rows[1][1], DocumentCell::Number(10_000.0));
    }

    #[test]
    fn corrupt_xlsx_and_xls_bytes_fail_explicitly() {
        for file_name in ["broken.xlsx", "broken.xls"] {
            let request = testing::request_for(file_name, b"not a spreadsheet at all");

            let outcome = ExcelOpener.open(&request);

            assert_eq!(
                outcome.load_state(),
                DocumentLoadState::Failed,
                "{file_name} must fail explicitly"
            );
        }
    }
}
