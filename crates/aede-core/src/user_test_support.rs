//! Shared in-memory catalog fixture for personal-data tests.

use crate::model::{self, Catalog};

pub(super) fn library(paths: &[&str]) -> Catalog {
    let files: Vec<_> = paths
        .iter()
        .map(|p| {
            model::tests::track(
                p,
                &[
                    ("title", "T"),
                    ("artist", "Deicide"),
                    ("album", "Legion"),
                    ("date", "1992"),
                ],
                1000,
            )
        })
        .collect();
    model::build(files, vec!["/m".into()], 0, &[])
}
