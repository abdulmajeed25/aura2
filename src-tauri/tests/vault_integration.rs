//! End-to-end test for Phase 1 vault operations: open, reindex, read, write, count.
//! Runs against a temporary copy of `tests/fixtures/sample-vault` so the fixture
//! stays pristine on disk.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::vault::VaultState;

fn copy_fixture_to_temp() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest
        .parent()
        .unwrap()
        .join("tests/fixtures/sample-vault");
    assert!(
        fixture.exists(),
        "fixture vault missing at {}",
        fixture.display()
    );

    let temp = std::env::temp_dir().join(format!("aura-test-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&temp).unwrap();
    copy_dir(&fixture, &temp).unwrap();
    temp
}

fn copy_dir(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn opens_indexes_reads_writes() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.expect("open vault");

    let report = vault.reindex().await.expect("reindex");
    assert_eq!(report.indexed, 3, "should index 3 markdown files");
    assert_eq!(report.skipped, 0);

    let count = vault.db.count_files().await.expect("count");
    assert_eq!(count, 3);

    // The Welcome note exists with its frontmatter title.
    let row = vault
        .db
        .get_file_by_path("Welcome.md")
        .await
        .expect("query")
        .expect("welcome row");
    assert_eq!(row.title, "Welcome to Aura");
    assert!(row.word_count > 0);
    assert!(row.frontmatter.as_deref().unwrap().contains("tags:"));

    // Write a new file via the vault layer and reindex one.
    let new_rel = "Drafts/NewNote.md";
    let new_abs = vault.resolve(new_rel).expect("resolve");
    fs::create_dir_all(new_abs.parent().unwrap()).unwrap();
    fs::write(&new_abs, "# Newly added\n\nbody text.").unwrap();
    vault.index_one(&new_abs).await.expect("index new file");

    let count_after = vault.db.count_files().await.expect("count");
    assert_eq!(count_after, 4);

    let new_row = vault
        .db
        .get_file_by_path(new_rel)
        .await
        .expect("query")
        .expect("new row");
    assert_eq!(new_row.title, "Newly added");

    // Cleanup
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn rejects_paths_escaping_vault_root() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.expect("open vault");

    assert!(vault.resolve("../escape.md").is_err());
    assert!(vault.resolve("/etc/passwd").is_err());
    assert!(vault.resolve("legit/file.md").is_ok());

    fs::remove_dir_all(&root).ok();
}
