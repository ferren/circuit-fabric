//! Read-only timings for the source-link navigation pipeline on a managed datasheet.
use std::{path::Path, time::Instant};

use circuitfabric_contracts::DatasheetExtraction;
use circuitfabric_project::ProjectStorage;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(
        arguments.len(),
        2,
        "usage: diagnose_source_navigation <project root> <document id>"
    );
    let storage = ProjectStorage::open(Path::new(&arguments[0])).expect("open project");
    let extraction =
        storage.load_datasheet_extraction(&arguments[1]).expect("load rows").expect("rows");
    let started = Instant::now();
    let request = storage
        .prepare_document_open(&storage.manifest().project.id, &arguments[1])
        .expect("verified copy");
    println!("integrity gate: {:?}", started.elapsed());
    let (section, evidence) = quotes(&extraction).next().expect("a source quote");
    let started = Instant::now();
    let cold_page = circuitfabric_document_opener::datasheet::locate_datasheet_evidence(
        &request, section, evidence,
    )
    .expect("cold lookup");
    println!("uncached source lookup: {:?}, page={cold_page:?}", started.elapsed());
    let document = storage
        .list_documents()
        .expect("document index")
        .into_iter()
        .find(|document| document.id == arguments[1])
        .expect("document");
    let started = Instant::now();
    let pages =
        circuitfabric_project::extract_pdf_pages(&storage, &document).expect("full search pages");
    println!("background text indexing: {:?}, {} pages", started.elapsed(), pages.len());
    let started = Instant::now();
    let index = circuitfabric_document_opener::datasheet::DatasheetEvidenceIndex::from_pages(
        &request.content_hash,
        &pages,
    );
    println!("prepare source index from existing pages: {:?}", started.elapsed());
    for (section, evidence) in quotes(&extraction).take(4) {
        let started = Instant::now();
        let page =
            index.locate(&request, section, evidence).expect("locate quote").expect("source page");
        let locate = started.elapsed();
        println!("{section}, page {page}: locate={locate:?}");
    }
    let started = Instant::now();
    let (mut found, mut missing) = (0, 0);
    for (section, evidence) in quotes(&extraction) {
        if index.locate(&request, section, evidence).expect("matching hash").is_some() {
            found += 1;
        } else {
            missing += 1;
        }
    }
    println!("all saved rows: {:?}, found={found}, missing={missing}", started.elapsed());
}

fn quotes(extraction: &DatasheetExtraction) -> impl Iterator<Item = (&str, &str)> {
    extraction
        .pins
        .iter()
        .filter_map(|row| row.evidence.as_deref().map(|text| ("pins", text)))
        .chain(
            extraction.absolute_maximum_ratings.iter().filter_map(|row| {
                row.evidence.as_deref().map(|text| ("absoluteMaximumRatings", text))
            }),
        )
        .chain(extraction.electrical_characteristics.iter().filter_map(|row| {
            row.evidence.as_deref().map(|text| ("electricalCharacteristics", text))
        }))
        .chain(
            extraction.operating_conditions.iter().filter_map(|row| {
                row.evidence.as_deref().map(|text| ("operatingConditions", text))
            }),
        )
}
