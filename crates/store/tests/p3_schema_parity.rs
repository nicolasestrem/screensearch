//! P3 audit: parity between the `03` section 4 DDL and the v14 baseline in
//! `crates/store/src/schema.rs`, plus the documented carried extras.
//!
//! `03` section 4 is the authority for every object it declares. It provably does not
//! describe every as-built object: the report tables, `frame_text_raw_fts`, a few older
//! carried columns / indexes and the FTS and vec sync triggers are absent from it, while
//! it still declares the legacy `ocr_text` / `ocr_text_fts` lane that v3 dropped. That
//! two-way gap is recorded as `specs/06` #30 (open doc gap), so this test pins both
//! directions instead of comparing object names only:
//!
//! - every `CREATE TABLE`, `CREATE VIRTUAL TABLE`, `CREATE INDEX` and
//!   `ALTER TABLE ... ADD COLUMN` that section 4 declares must exist in the baseline with
//!   the same columns, constraints, index columns and virtual-table arguments (tokens are
//!   compared after stripping comments, so formatting and the section's prose comments
//!   cannot hide a real difference);
//! - the three documented extra table definitions (`frames.image_purged`,
//!   `text_spans.line_index`, `embeddings` UNIQUE) must be present, not merely allowed;
//! - the two section 4 declarations the as-built baseline deliberately lacks must stay
//!   absent, so a stray re-introduction of the dropped OCR lane is caught;
//! - the documented carried extras must exist, and nothing undocumented may be added to
//!   either side. Section 4 declares **no** `CREATE TRIGGER` (it names FTS/vec sync only
//!   in prose), so the seven carried triggers are existence-checked by name here; their
//!   bodies are covered by the functional FTS and `vec0` sync tests;
//! - opening a fresh store must materialise exactly the baseline script.
//!
//! No row count and no table count is hardcoded. The section 4 side is parsed out of the
//! spec at compile time; the extras below are the documented `specs/06` #30 list plus the
//! section 4 prose FTS/vec triggers, not a snapshot of whatever the schema currently
//! contains.

use std::collections::BTreeMap;

use rusqlite::Connection;
use store::{SqliteStore, BASELINE_SQL, LATEST_SCHEMA_VERSION};

/// The master spec, read only for the section 4 DDL block.
const SPEC_03: &str = include_str!("../../../specs/03_MASTER_PRODUCTION_SPEC.md");

/// `03` section 4 still declares these, but v3 dropped the legacy OCR lane and the
/// as-built v14 baseline deliberately does not create them (`specs/06` #30). The test
/// pins their absence: recreating either is schema drift, not a parity fix.
const SPEC_ONLY_ABSENT: [&str; 2] = ["ocr_text", "ocr_text_fts"];

/// Tables `03` section 4 does not declare but the as-built baseline carries unchanged
/// (`specs/06` #30; P3.md "Baseline schema": the v14 schema is v13 plus the eleven new
/// tables). The key columns asserted for each keep this stronger than a name-only check.
const CARRIED_TABLES: [(&str, &[&str]); 3] = [
    (
        "report_runs",
        &[
            "run_id",
            "schedule_id",
            "revision_of",
            "window_start",
            "window_end",
            "phase",
            "created_at",
            "updated_at",
            "payload",
            "kind",
        ],
    ),
    (
        "report_deliveries",
        &["id", "run_id", "state", "next_attempt_at", "payload"],
    ),
    ("report_files", &["path", "payload"]),
];

/// The raw-text FTS mirror `03` section 4 describes only in prose.
const CARRIED_VIRTUAL_TABLES: [&str; 1] = ["frame_text_raw_fts"];

/// Indexes `03` section 4 does not declare but the baseline carries: two older as-built
/// indexes and the four report indexes.
const CARRIED_INDEXES: [&str; 6] = [
    "idx_frames_image_retention",
    "idx_jobs_frame_kind_state",
    "idx_report_schedule_window",
    "idx_report_runs_created",
    "idx_report_runs_kind_created",
    "idx_report_deliveries_run",
];

/// Triggers `03` section 4 mentions only in prose ("triggers keep FTS in sync"), plus
/// the vec0 shadow cleanup trigger.
const CARRIED_TRIGGERS: [&str; 7] = [
    "frame_text_ai",
    "frame_text_ad",
    "frame_text_au",
    "frame_text_raw_ai",
    "frame_text_raw_ad",
    "frame_text_raw_au",
    "embeddings_ad",
];

/// The section 4 DDL block: the first ```sql fence after the section 4 heading and before
/// the next `## ` heading, read up to the first closing fence. The rest of the spec is
/// never touched.
fn section_4_ddl(spec: &str) -> &str {
    const HEADING: &str = "## 4. Data model (";
    let at = spec
        .find(HEADING)
        .unwrap_or_else(|| panic!("03 has no section 4 heading starting {HEADING:?}"));
    let after_heading = &spec[at..];
    let scope_end = after_heading.find("\n## ").unwrap_or(after_heading.len());
    let scope = &after_heading[..scope_end];
    let fence = "```sql";
    let fence_at = scope
        .find(fence)
        .unwrap_or_else(|| panic!("03 section 4 has no {fence:?} DDL block"));
    let after_fence = &scope[fence_at + fence.len()..];
    let end = after_fence
        .find("\n```")
        .unwrap_or_else(|| panic!("03 section 4 DDL block is unterminated"));
    &after_fence[..end]
}

/// Strips `--` line comments, then splits into lowercased DDL tokens. A quoted string
/// stays one token; every other run is either an identifier/keyword (lowercased) or a
/// single punctuation character. Comments contain no `--` inside a literal in this DDL.
fn ddl_tokens(sql: &str) -> Vec<String> {
    let uncommented: String = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let chars: Vec<char> = uncommented.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    i += 1;
                    if i < chars.len() && chars[i] == '\'' {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
        } else if c.is_ascii_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(
                chars[start..i]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
            );
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
}

/// `ddl_tokens` joined with single spaces: the comparable form of a definition or of a
/// whole statement.
fn norm(sql: &str) -> String {
    ddl_tokens(sql).join(" ")
}

/// Splits tokenized DDL into statements. `;` only separates statements at the top level
/// here (trigger bodies are the one place a semicolon appears inside a statement, so
/// their segments are merged back up to the closing `END`).
fn ddl_statements(sql: &str) -> Vec<Vec<String>> {
    let mut raw: Vec<Vec<String>> = Vec::new();
    for statement in ddl_tokens(sql).split(|t| t == ";") {
        if !statement.is_empty() {
            raw.push(statement.to_vec());
        }
    }
    let mut statements: Vec<Vec<String>> = Vec::new();
    let mut iter = raw.into_iter();
    while let Some(first) = iter.next() {
        if first.len() >= 2 && first[0] == "create" && first[1] == "trigger" {
            let mut trigger = first;
            while trigger.last().map(String::as_str) != Some("end") {
                match iter.next() {
                    Some(next) => trigger.extend(next),
                    None => break,
                }
            }
            statements.push(trigger);
        } else {
            statements.push(first);
        }
    }
    statements
}

/// The comma-separated definitions inside a `CREATE TABLE` body, each normalized.
fn table_defs(statement: &[String]) -> Vec<String> {
    let open = statement
        .iter()
        .position(|t| t == "(")
        .unwrap_or_else(|| panic!("CREATE TABLE has no column list: {statement:?}"));
    let close = statement
        .iter()
        .rposition(|t| t == ")")
        .unwrap_or_else(|| panic!("CREATE TABLE has no closing paren: {statement:?}"));
    let mut defs: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut depth: usize = 0;
    for token in &statement[open + 1..close] {
        match token.as_str() {
            "(" => {
                depth += 1;
                current.push(token.clone());
            }
            ")" => {
                depth = depth.saturating_sub(1);
                current.push(token.clone());
            }
            "," if depth == 0 => {
                if !current.is_empty() {
                    defs.push(current.join(" "));
                    current.clear();
                }
            }
            _ => current.push(token.clone()),
        }
    }
    if !current.is_empty() {
        defs.push(current.join(" "));
    }
    defs
}

/// One parsed DDL script: table definitions (with `ALTER TABLE ... ADD COLUMN`
/// definitions folded in), virtual tables, indexes and triggers, all normalized.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Ddl {
    tables: BTreeMap<String, Vec<String>>,
    virtual_tables: BTreeMap<String, String>,
    indexes: BTreeMap<String, String>,
    triggers: BTreeMap<String, String>,
}

fn parse_ddl(sql: &str) -> Ddl {
    let mut ddl = Ddl::default();
    for statement in ddl_statements(sql) {
        let kind = statement.get(1).map(String::as_str);
        match (statement.first().map(String::as_str), kind) {
            (Some("create"), Some("table")) => {
                let name = statement
                    .get(2)
                    .unwrap_or_else(|| panic!("CREATE TABLE has no name: {statement:?}"))
                    .clone();
                let defs = table_defs(&statement);
                assert!(
                    ddl.tables.insert(name.clone(), defs).is_none(),
                    "two CREATE TABLE statements for `{name}`"
                );
            }
            (Some("create"), Some("virtual")) => {
                assert_eq!(
                    statement.get(2).map(String::as_str),
                    Some("table"),
                    "CREATE VIRTUAL must be CREATE VIRTUAL TABLE: {statement:?}"
                );
                let name = statement
                    .get(3)
                    .unwrap_or_else(|| panic!("CREATE VIRTUAL TABLE has no name: {statement:?}"))
                    .clone();
                assert!(
                    ddl.virtual_tables
                        .insert(name.clone(), statement.join(" "))
                        .is_none(),
                    "two CREATE VIRTUAL TABLE statements for `{name}`"
                );
            }
            (Some("create"), Some("index")) => {
                let name = statement
                    .get(2)
                    .unwrap_or_else(|| panic!("CREATE INDEX has no name: {statement:?}"))
                    .clone();
                assert!(
                    ddl.indexes
                        .insert(name.clone(), statement.join(" "))
                        .is_none(),
                    "two CREATE INDEX statements for `{name}`"
                );
            }
            (Some("create"), Some("unique")) => {
                assert_eq!(
                    statement.get(2).map(String::as_str),
                    Some("index"),
                    "CREATE UNIQUE must be CREATE UNIQUE INDEX: {statement:?}"
                );
                let name = statement
                    .get(3)
                    .unwrap_or_else(|| panic!("CREATE UNIQUE INDEX has no name: {statement:?}"))
                    .clone();
                assert!(
                    ddl.indexes
                        .insert(name.clone(), statement.join(" "))
                        .is_none(),
                    "two CREATE INDEX statements for `{name}`"
                );
            }
            (Some("create"), Some("trigger")) => {
                let name = statement
                    .get(2)
                    .unwrap_or_else(|| panic!("CREATE TRIGGER has no name: {statement:?}"))
                    .clone();
                assert!(
                    ddl.triggers
                        .insert(name.clone(), statement.join(" "))
                        .is_none(),
                    "two CREATE TRIGGER statements for `{name}`"
                );
            }
            (Some("alter"), Some("table")) => {
                let name = statement
                    .get(2)
                    .unwrap_or_else(|| panic!("ALTER TABLE has no name: {statement:?}"))
                    .clone();
                assert_eq!(
                    statement.get(3).map(String::as_str),
                    Some("add"),
                    "only ALTER TABLE ... ADD COLUMN is expected: {statement:?}"
                );
                assert_eq!(
                    statement.get(4).map(String::as_str),
                    Some("column"),
                    "only ALTER TABLE ... ADD COLUMN is expected: {statement:?}"
                );
                let def = statement[5..].join(" ");
                ddl.tables
                    .get_mut(&name)
                    .unwrap_or_else(|| panic!("ALTER TABLE `{name}` before its CREATE TABLE"))
                    .push(def);
            }
            _ => panic!("unexpected DDL statement: {statement:?}"),
        }
    }
    ddl
}

/// Extra definitions inside section 4 tables that the spec does not declare but the
/// as-built baseline carries (`specs/06` #30). Kept as documented literals so an
/// unrelated column cannot slip in unnoticed.
fn documented_extra_defs(table: &str) -> Vec<String> {
    match table {
        "frames" => vec![norm(
            "image_purged INTEGER NOT NULL DEFAULT 0 CHECK (image_purged IN (0,1))",
        )],
        "text_spans" => vec![norm("line_index INTEGER NOT NULL DEFAULT 0")],
        "embeddings" => vec![norm("UNIQUE(frame_id, chunk_index)")],
        _ => Vec::new(),
    }
}

/// Source-level parity: the baseline script must declare every DDL object of `03`
/// section 4 (including the column an ALTER adds), keep the two documented dropped
/// declarations absent, and carry exactly the documented extras.
#[test]
fn baseline_holds_every_spec_section_4_declaration_and_the_documented_extras() {
    let spec = parse_ddl(section_4_ddl(SPEC_03));
    let base = parse_ddl(BASELINE_SQL);

    // Sanity: the parsed spec block really is a DDL script, not a stray fence.
    assert!(
        !spec.tables.is_empty() && !spec.indexes.is_empty(),
        "parsed no tables/indexes from 03 section 4; the extraction or the spec changed"
    );
    // Sentinel objects: a block truncated or shifted to some earlier snippet cannot pass
    // by parsing an empty or unrelated fragment.
    assert!(
        spec.tables.contains_key("frames") && spec.tables.contains_key("schema_version"),
        "03 section 4 extraction lost its sentinel tables"
    );
    assert!(
        spec.indexes.contains_key("idx_frames_captured_at"),
        "03 section 4 extraction lost its sentinel index"
    );

    for (name, spec_defs) in &spec.tables {
        if SPEC_ONLY_ABSENT.contains(&name.as_str()) {
            assert!(
                !base.tables.contains_key(name),
                "`{name}` is declared by 03 section 4 but v3 dropped it; the baseline must \
                 not recreate it (specs/06 #30)"
            );
            continue;
        }
        let base_defs = base.tables.get(name).unwrap_or_else(|| {
            panic!("03 section 4 declares table `{name}` but the v14 baseline does not create it")
        });
        let documented = documented_extra_defs(name);
        // The documented as-built extras are required, not just tolerated: dropping one
        // from schema.rs while leaving the rest of the table in place must fail here.
        for def in &documented {
            assert!(
                base_defs.contains(def),
                "table `{name}`: the documented carried extra `{def}` (specs/06 #30) is \
                 missing from the v14 baseline"
            );
        }
        for def in spec_defs {
            assert!(
                base_defs.iter().any(|have| have == def),
                "table `{name}`: 03 section 4 declares `{def}` but the baseline does not \
                 contain that definition"
            );
        }
        for def in base_defs {
            assert!(
                spec_defs.contains(def) || documented.contains(def),
                "table `{name}`: the baseline has `{def}`, which neither 03 section 4 nor \
                 the documented carried extras (specs/06 #30) declare"
            );
        }
    }
    for name in base.tables.keys() {
        if !spec.tables.contains_key(name) {
            assert!(
                CARRIED_TABLES.iter().any(|(carried, _)| carried == name),
                "the baseline creates table `{name}`, which is neither declared by 03 \
                 section 4 nor a documented carried table (specs/06 #30)"
            );
        }
    }
    for (name, key_columns) in CARRIED_TABLES {
        let defs = base.tables.get(name).unwrap_or_else(|| {
            panic!("documented carried table `{name}` is missing from the v14 baseline")
        });
        for column in key_columns {
            assert!(
                defs.iter()
                    .any(|def| def.split(' ').next() == Some(*column)),
                "documented carried table `{name}` is missing its `{column}` column"
            );
        }
    }

    for (name, spec_statement) in &spec.virtual_tables {
        if SPEC_ONLY_ABSENT.contains(&name.as_str()) {
            assert!(
                !base.virtual_tables.contains_key(name),
                "`{name}` is declared by 03 section 4 but v3 dropped it; the baseline must \
                 not recreate it (specs/06 #30)"
            );
            continue;
        }
        let base_statement = base.virtual_tables.get(name).unwrap_or_else(|| {
            panic!(
                "03 section 4 declares virtual table `{name}` but the v14 baseline does not \
                 create it"
            )
        });
        assert_eq!(
            base_statement, spec_statement,
            "virtual table `{name}` differs from 03 section 4"
        );
    }
    for name in base.virtual_tables.keys() {
        if !spec.virtual_tables.contains_key(name) {
            assert!(
                CARRIED_VIRTUAL_TABLES.contains(&name.as_str()),
                "the baseline creates virtual table `{name}`, which is neither declared by \
                 03 section 4 nor a documented carried extra (specs/06 #30)"
            );
        }
    }
    for name in CARRIED_VIRTUAL_TABLES {
        assert!(
            base.virtual_tables.contains_key(name),
            "documented carried virtual table `{name}` is missing from the v14 baseline"
        );
    }

    for (name, spec_statement) in &spec.indexes {
        let base_statement = base.indexes.get(name).unwrap_or_else(|| {
            panic!("03 section 4 declares index `{name}` but the v14 baseline does not create it")
        });
        assert_eq!(
            base_statement, spec_statement,
            "index `{name}` differs from 03 section 4"
        );
    }
    for name in base.indexes.keys() {
        if !spec.indexes.contains_key(name) {
            assert!(
                CARRIED_INDEXES.contains(&name.as_str()),
                "the baseline creates index `{name}`, which is neither declared by 03 \
                 section 4 nor a documented carried extra (specs/06 #30)"
            );
        }
    }
    for name in CARRIED_INDEXES {
        assert!(
            base.indexes.contains_key(name),
            "documented carried index `{name}` is missing from the v14 baseline"
        );
    }

    for (name, spec_statement) in &spec.triggers {
        let base_statement = base.triggers.get(name).unwrap_or_else(|| {
            panic!("03 section 4 declares trigger `{name}` but the v14 baseline does not create it")
        });
        assert_eq!(
            base_statement, spec_statement,
            "trigger `{name}` differs from 03 section 4"
        );
    }
    for name in base.triggers.keys() {
        if !spec.triggers.contains_key(name) {
            assert!(
                CARRIED_TRIGGERS.contains(&name.as_str()),
                "the baseline creates trigger `{name}`, which is neither declared by 03 \
                 section 4 nor a documented carried extra (specs/06 #30)"
            );
        }
    }
    for name in CARRIED_TRIGGERS {
        assert!(
            base.triggers.contains_key(name),
            "documented carried trigger `{name}` is missing from the v14 baseline"
        );
    }
}

/// Runtime parity: opening a fresh store must materialise exactly the baseline script
/// (same tables, columns, constraints, indexes, virtual tables and triggers). This is
/// what proves the checked source script is the schema the app actually creates, not a
/// dead string.
#[test]
fn a_fresh_store_materialises_exactly_the_baseline_ddl() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("screensearch.db");
    {
        let store = SqliteStore::open_path(&path).expect("create the v14 baseline");
        assert_eq!(
            store.schema_version().expect("read schema_version"),
            LATEST_SCHEMA_VERSION
        );
    }

    // The store has been closed. `open_path` registered the FTS5/vec extensions for the
    // process, so a plain connection can read what the baseline created.
    let conn = Connection::open(&path).expect("re-open the baseline file");
    let base = parse_ddl(BASELINE_SQL);
    let shadow_prefixes: Vec<String> = base
        .virtual_tables
        .keys()
        .map(|name| format!("{name}_"))
        .collect();

    let mut stmt = conn
        .prepare(
            "SELECT type, name, sql FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .expect("prepare sqlite_master query");
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .expect("query sqlite_master")
        .collect::<rusqlite::Result<_>>()
        .expect("read sqlite_master rows");

    let mut actual = Ddl::default();
    for (kind, name, sql) in rows {
        // FTS5 / vec0 shadow tables are an implementation detail of the extension.
        if shadow_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let sql = sql.unwrap_or_else(|| panic!("`{name}` has no CREATE statement"));
        let parsed = parse_ddl(&sql);
        match kind.as_str() {
            "table" => {
                if let Some(defs) = parsed.tables.get(&name) {
                    actual.tables.insert(name, defs.clone());
                } else if let Some(statement) = parsed.virtual_tables.get(&name) {
                    actual.virtual_tables.insert(name, statement.clone());
                } else {
                    panic!("could not classify created table `{name}`");
                }
            }
            "index" => {
                let statement = parsed
                    .indexes
                    .get(&name)
                    .unwrap_or_else(|| panic!("could not parse created index `{name}`"));
                actual.indexes.insert(name, statement.clone());
            }
            "trigger" => {
                let statement = parsed
                    .triggers
                    .get(&name)
                    .unwrap_or_else(|| panic!("could not parse created trigger `{name}`"));
                actual.triggers.insert(name, statement.clone());
            }
            other => panic!("unexpected sqlite_master object type {other:?} named `{name}`"),
        }
    }

    assert_eq!(
        actual.tables, base.tables,
        "the created database's tables differ from the baseline script"
    );
    assert_eq!(
        actual.virtual_tables, base.virtual_tables,
        "the created database's virtual tables differ from the baseline script"
    );
    assert_eq!(
        actual.indexes, base.indexes,
        "the created database's indexes differ from the baseline script"
    );
    assert_eq!(
        actual.triggers, base.triggers,
        "the created database's triggers differ from the baseline script"
    );
}
