use crate::model::{AnnoLichess, GameId, GameInfo, PersonalEntry, PersonalKey, PersonalKeyPrefix};
use rocksdb::{ColumnFamilyDescriptor, DBWithThreadMode, MergeOperands, Options};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug)]
pub struct Database {
    inner: DBWithThreadMode<rocksdb::SingleThreaded>,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Database, rocksdb::Error> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        let mut personal_opts = Options::default();
        personal_opts.set_merge_operator_associative("personal merge", personal_merge);

        let mut game_opts = Options::default();
        game_opts.set_merge_operator_associative("game merge", game_merge);

        let inner = DBWithThreadMode::open_cf_descriptors(
            &db_opts,
            path,
            vec![
                ColumnFamilyDescriptor::new("personal", personal_opts),
                ColumnFamilyDescriptor::new("game", game_opts),
            ],
        )?;

        Ok(Database { inner })
    }

    pub fn queryable(&self) -> QueryableDatabase<'_> {
        QueryableDatabase {
            db: &self.inner,
            cf_personal: self.inner.cf_handle("personal").expect("cf personal"),
            cf_game: self.inner.cf_handle("game").expect("cf game"),
        }
    }
}

pub struct QueryableDatabase<'a> {
    pub db: &'a DBWithThreadMode<rocksdb::SingleThreaded>,
    pub cf_personal: &'a rocksdb::ColumnFamily,
    pub cf_game: &'a rocksdb::ColumnFamily,
}

impl QueryableDatabase<'_> {
    pub fn merge_game_info(&self, id: GameId, info: GameInfo) -> Result<(), rocksdb::Error> {
        let mut cursor = Cursor::new(Vec::new());
        info.write(&mut cursor).expect("serialize game info");
        self.db
            .merge_cf(self.cf_game, id.to_bytes(), cursor.into_inner())
    }

    pub fn get_game_info(&self, id: GameId) -> Result<Option<GameInfo>, rocksdb::Error> {
        Ok(self.db.get_cf(self.cf_game, id.to_bytes())?.map(|buf| {
            let mut cursor = Cursor::new(buf);
            GameInfo::read(&mut cursor).expect("deserialize game info")
        }))
    }

    pub fn merge_personal(
        &self,
        key: PersonalKey,
        entry: PersonalEntry,
    ) -> Result<(), rocksdb::Error> {
        let mut cursor = Cursor::new(Vec::new());
        entry.write(&mut cursor).expect("serialize personal entry");
        self.db
            .merge_cf(self.cf_personal, key.into_bytes(), cursor.into_inner())
    }

    pub fn get_personal(
        &self,
        key: PersonalKeyPrefix,
        since: AnnoLichess,
    ) -> Result<PersonalEntry, rocksdb::Error> {
        let mut entry = PersonalEntry::default();

        let mut end = rocksdb::ReadOptions::default();
        end.set_iterate_upper_bound(key.with_year(AnnoLichess::MAX).into_bytes());
        let iterator = self.db.iterator_cf_opt(
            self.cf_personal,
            end,
            rocksdb::IteratorMode::From(
                &key.with_year(since).into_bytes(),
                rocksdb::Direction::Forward,
            ),
        );

        for (_key, value) in iterator {
            let mut cursor = Cursor::new(value);
            entry
                .extend_from_reader(&mut cursor)
                .expect("deserialize personal entry");
        }

        Ok(entry)
    }
}

fn game_merge(
    _key: &[u8],
    existing: Option<&[u8]>,
    operands: &mut MergeOperands,
) -> Option<Vec<u8>> {
    // Take latest game info, but merge index status.
    let mut info: Option<GameInfo> = None;
    for op in existing.into_iter().chain(operands.into_iter()) {
        let mut cursor = Cursor::new(op);
        let mut new_info = GameInfo::read(&mut cursor).expect("read for game merge");
        if let Some(old_info) = info {
            new_info.indexed.white |= old_info.indexed.white;
            new_info.indexed.black |= old_info.indexed.black;
        }
        info = Some(new_info);
    }
    info.map(|info| {
        let mut cursor = Cursor::new(Vec::new());
        info.write(&mut cursor).expect("write game");
        cursor.into_inner()
    })
}

fn personal_merge(
    _key: &[u8],
    existing: Option<&[u8]>,
    operands: &mut MergeOperands,
) -> Option<Vec<u8>> {
    let mut entry = PersonalEntry::default();
    let mut size_hint = 0;
    for op in existing.into_iter().chain(operands.into_iter()) {
        let mut cursor = Cursor::new(op);
        entry
            .extend_from_reader(&mut cursor)
            .expect("deserialize for personal merge");
        size_hint += op.len();
    }
    let mut cursor = Cursor::new(Vec::with_capacity(size_hint));
    entry.write(&mut cursor).expect("write personal entry");
    Some(cursor.into_inner())
}
