//! Phase 12: canvas read/write/list round-trip against a real vault.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::canvas::{CanvasDoc, CanvasEdge, CanvasNode};
use aura_lib::core::vault::VaultState;
use uuid::Uuid;

fn copy_fixture_to_temp() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest
        .parent()
        .unwrap()
        .join("tests/fixtures/sample-vault");
    let temp = std::env::temp_dir().join(format!("aura-test-{}", Uuid::now_v7()));
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
async fn canvas_round_trip_through_disk() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();

    let canvas_path = vault.resolve("Boards/Roadmap.canvas").unwrap();
    fs::create_dir_all(canvas_path.parent().unwrap()).unwrap();

    let doc = CanvasDoc {
        nodes: vec![
            CanvasNode::File {
                id: "n1".into(),
                x: 0.0,
                y: 0.0,
                width: 320.0,
                height: 220.0,
                file: "Welcome.md".into(),
            },
            CanvasNode::Text {
                id: "n2".into(),
                x: 380.0,
                y: 40.0,
                width: 260.0,
                height: 160.0,
                text: "Ship Phase 12 then polish.".into(),
            },
        ],
        edges: vec![CanvasEdge {
            id: "e1".into(),
            from_node: "n1".into(),
            to_node: "n2".into(),
            label: Some("next".into()),
        }],
    };

    let serialized = doc.serialize_pretty().unwrap();
    fs::write(&canvas_path, &serialized).unwrap();

    let back = CanvasDoc::parse(&fs::read_to_string(&canvas_path).unwrap()).unwrap();
    assert_eq!(back.nodes.len(), 2);
    assert_eq!(back.edges.len(), 1);
    assert_eq!(back.edges[0].label.as_deref(), Some("next"));
    back.validate().unwrap();

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn canvas_walker_finds_canvas_files_only() {
    let root = copy_fixture_to_temp();
    let _vault = VaultState::open(root.clone()).await.unwrap();

    // Drop two canvas files and one stray .json — only the canvases should
    // count toward what the command would return.
    let c1 = root.join("a.canvas");
    let c2 = root.join("sub/b.canvas");
    fs::create_dir_all(c2.parent().unwrap()).unwrap();
    fs::write(&c1, "{}").unwrap();
    fs::write(&c2, "{}").unwrap();
    fs::write(root.join("not-a-canvas.json"), "{}").unwrap();

    let mut found: Vec<String> = Vec::new();
    for entry in ignore::WalkBuilder::new(&root).build().flatten() {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("canvas") {
            found.push(p.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    found.sort();
    assert_eq!(found, vec!["a.canvas".to_string(), "b.canvas".to_string()]);

    fs::remove_dir_all(&root).ok();
}
