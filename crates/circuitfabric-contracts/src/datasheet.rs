//! Structured datasheet extraction contracts.
//!
//! A [`DatasheetExtraction`] is the circuit-design-relevant projection of one managed
//! datasheet document: part identity, pinout, and the parameter tables (absolute maximum
//! ratings, electrical characteristics, operating conditions). It is derived metadata for
//! engineers and UIs, always tied to the exact content hash of the document it was
//! extracted from. Only rows carrying a verified source line (`evidence`) may be surfaced
//! as evidence, and then only that verbatim line, never the derived cells.

use serde::{Deserialize, Serialize};

use crate::DocumentId;

/// Version of the persisted datasheet-extraction document.
pub const DATASHEET_EXTRACTION_SCHEMA_VERSION: u32 = 1;

/// Upper bounds applied while extracting, so a hostile document cannot balloon the store.
pub const MAX_DATASHEET_PINS: usize = 256;
pub const MAX_DATASHEET_PARAMETERS: usize = 128;
pub const MAX_DATASHEET_FEATURES: usize = 32;

/// The structured projection of one datasheet.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetExtraction {
    pub schema_version: u32,
    pub document_id: DocumentId,
    /// Content hash of the exact bytes this extraction was made from; mismatch with the
    /// live document means the extraction is stale and must be regenerated.
    pub content_hash: String,
    pub extracted_at_unix_seconds: u64,
    pub overview: DatasheetOverview,
    pub pins: Vec<DatasheetPin>,
    pub absolute_maximum_ratings: Vec<DatasheetParameter>,
    pub electrical_characteristics: Vec<DatasheetParameter>,
    pub operating_conditions: Vec<DatasheetParameter>,
    /// Parser diagnostics: which expected sections were not found, and why values may be
    /// partial. Surfaced to the user instead of silently returning half a table.
    pub notes: Vec<String>,
}

/// Identity and narrative fields of a datasheet.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetOverview {
    /// Document title line, usually the part name plus a short function description.
    pub title: String,
    /// Part numbers detected in the title and headers (e.g. `LM317`, `TPS7A2033`).
    pub part_numbers: Vec<String>,
    /// Detected manufacturer, when a known vendor name appears on the early pages.
    pub manufacturer: Option<String>,
    /// Detected package names (e.g. `SOT-23`, `SOIC-8`, `TO-220`).
    pub packages: Vec<String>,
    /// Feature bullet points from the Features section.
    pub features: Vec<String>,
    /// First paragraphs of the Description section, capped.
    pub description: String,
}

/// The functional role of one pin, classified from its name and type column.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DatasheetPinKind {
    Power,
    Ground,
    Input,
    Output,
    InputOutput,
    NotConnected,
    Other,
}

impl DatasheetPinKind {
    /// Short display label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Power => "power",
            Self::Ground => "ground",
            Self::Input => "input",
            Self::Output => "output",
            Self::InputOutput => "I/O",
            Self::NotConnected => "n.c.",
            Self::Other => "other",
        }
    }
}

/// One row of the pin description table.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetPin {
    /// Pin identifier: plain numbers, but also grid refs like `A1`.
    pub number: String,
    /// Symbolic pin name, e.g. `VIN`, `EN`, `OUT`, `GND`.
    pub name: String,
    pub kind: DatasheetPinKind,
    pub description: String,
    /// The source line this row was verified against (whitespace-normalized). Only
    /// agent extractions checked against the PDF text and by Jev carry it; such rows
    /// may be surfaced as evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// One row of a parameter table (ratings, characteristics, conditions).
///
/// Values stay strings: datasheet cells hold ranges (`-0.3`, `26`), signed markers
/// (`±30`), and footnote markers that do not reduce losslessly to numbers.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetParameter {
    /// Human-readable parameter name, e.g. `Input voltage`.
    pub parameter: String,
    /// Symbol column, e.g. `VIN`, `IOUT`.
    pub symbol: Option<String>,
    pub min: Option<String>,
    pub typ: Option<String>,
    pub max: Option<String>,
    /// Unit of the numeric columns, e.g. `V`, `mA`, `°C`.
    pub unit: Option<String>,
    /// Test or usage conditions attached to the row, when present.
    pub conditions: Option<String>,
    /// The verified source line; see [`DatasheetPin::evidence`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_round_trips_through_json() {
        let extraction = DatasheetExtraction {
            schema_version: DATASHEET_EXTRACTION_SCHEMA_VERSION,
            document_id: "doc-abc123-1".to_owned(),
            content_hash: "sha256:00".to_owned(),
            extracted_at_unix_seconds: 5,
            overview: DatasheetOverview {
                title: "LM317 3-Terminal Adjustable Regulator".to_owned(),
                part_numbers: vec!["LM317".to_owned()],
                manufacturer: Some("Texas Instruments".to_owned()),
                packages: vec!["TO-220".to_owned()],
                features: vec!["Output voltage adjustable".to_owned()],
                description: "The LM317 is an adjustable positive regulator.".to_owned(),
            },
            pins: vec![DatasheetPin {
                number: "3".to_owned(),
                name: "VIN".to_owned(),
                kind: DatasheetPinKind::Power,
                description: "Input power supply".to_owned(),
                evidence: Some("3 VIN Input power supply".to_owned()),
            }],
            absolute_maximum_ratings: vec![DatasheetParameter {
                parameter: "Input voltage".to_owned(),
                symbol: Some("VIN".to_owned()),
                min: None,
                typ: None,
                max: Some("40".to_owned()),
                unit: Some("V".to_owned()),
                conditions: None,
                evidence: None,
            }],
            electrical_characteristics: Vec::new(),
            operating_conditions: Vec::new(),
            notes: vec!["pin table found on page 2".to_owned()],
        };

        let json = serde_json::to_string(&extraction).expect("serialize");
        let decoded: DatasheetExtraction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, extraction);
        assert!(json.contains("\"absoluteMaximumRatings\""));
        assert!(json.contains("\"schemaVersion\""));
    }
}
