use ferrodb::index::btree::BTree;
use ferrodb::storage::buffer::BufferPool;
use ferrodb::storage::disk::{DiskManager, FileDiskManager};
use ferrodb::storage::heap::RowId;

#[test]
fn tree_in_a_file_finds_keys_and_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let disk: Box<dyn DiskManager> =
        Box::new(FileDiskManager::create(&dir.path().join("t.index")).unwrap());
    let pool = BufferPool::new(disk, 4);
    let mut tree = BTree::<i32>::create(&pool).unwrap();
    for key in 0..2000 {
        let row_id = RowId {
            page: usize::try_from(key).unwrap(),
            slot: 0,
        };
        tree.insert(key, row_id).unwrap();
    }
    assert_eq!(tree.get(&42).unwrap(), Some(RowId { page: 42, slot: 0 }));
    let entries: Vec<(i32, RowId)> = tree
        .range(10..13)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        entries,
        vec![
            (10, RowId { page: 10, slot: 0 }),
            (11, RowId { page: 11, slot: 0 }),
            (12, RowId { page: 12, slot: 0 }),
        ]
    );
    drop(pool);
    let size = std::fs::metadata(dir.path().join("t.index")).unwrap().len();
    assert!(size > 3 * 8192);
}
