//! Data-driven personas for `ssv2c-evalfixture gen`.
//!
//! A **persona** is one data definition: who the synthetic user is, where and how they work,
//! which tools they use and what they are working on. The generator has no per-persona code
//! paths: `set_a.rs`, `set_b.rs` and `set_c.rs` hold only [`Persona`] literals, and every
//! generator decision is derived from those values by `gen::life` (timeline and sessions) and
//! `gen::render` (screens). Adding a persona later means adding one literal to one of those
//! files - no new Rust logic.
//!
//! Every persona is a **Windows desktop user**: ScreenSearch is a Windows-only product
//! (`CLAUDE.md` hard rule), so the captured environment is always Windows. The personas differ
//! in occupation, language, country, timezone, rhythm, tools and content, never in platform.
//!
//! The catalog is **seed-independent** in the same way the default `world` catalog is: the
//! same personas exist for every seed, and the seed only decides when, where and how often
//! the persona's own material appears. Every string in this module and its submodules is
//! invented. Well-known applications and public sites may be named; employers, people,
//! documents, projects, intranet sites and figures are synthetic, and no definition carries
//! banking, medical or intimate content.
//!
//! Personas are validated structurally before use ([`validate_all`]): unique ids, every
//! activity backed by an app of that kind, non-empty content lists, well-formed week ranges
//! and offsets. A malformed definition fails the run with the persona id in the message
//! instead of producing a mostly empty fixture.

mod labels;
mod set_a;
mod set_b;
mod set_c;

pub(crate) use labels::*;

use anyhow::{ensure, Context as _, Result};
use chrono::FixedOffset;
use serde::{Deserialize, Serialize};

/// Settings key carrying the generated persona's identity in the fixture database. The
/// `export` command reads it back; a database without it (the pinned default fixture) is
/// exported with [`legacy_metadata`].
pub(crate) const PERSONA_SETTING: &str = "evalfixture.persona";

/// One attached display, in pixels. A persona has one or two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Monitor {
    pub width: u32,
    pub height: u32,
}

/// A foreground application of the persona's life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppDef {
    pub kind: AppKind,
    /// Display name as the window title and menus show it, in the persona's language where
    /// the product is localized (the app *names* themselves stay the well-known Windows ones).
    pub name: &'static str,
    /// Executable stem, as `app_hint` records it on Windows (for example `EXCEL`, `WINWORD`,
    /// `chrome`).
    pub hint: &'static str,
    /// Relative usage weight inside the persona's app set, 1..=100.
    pub weight: u64,
}

/// The behaviour family of an application: which screen renderer draws it and which
/// activities can open it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppKind {
    Code,
    Terminal,
    Browser,
    Mail,
    Spreadsheet,
    Document,
    Slides,
    Pdf,
    Chat,
    Files,
    Meeting,
    Notes,
    /// A visual workspace: music production, photo or video editing, CAD.
    Canvas,
    Game,
}

impl AppKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Terminal => "terminal",
            Self::Browser => "browser",
            Self::Mail => "mail",
            Self::Spreadsheet => "spreadsheet",
            Self::Document => "document",
            Self::Slides => "slides",
            Self::Pdf => "pdf",
            Self::Chat => "chat",
            Self::Files => "files",
            Self::Meeting => "meeting",
            Self::Notes => "notes",
            Self::Canvas => "canvas",
            Self::Game => "game",
        }
    }
}

/// The weekly rhythm of the persona's screen time.
///
/// `patterns` is a cycle of weeks (indexed by `week % patterns.len()`), each week listing
/// the weekday indexes that hold sessions (`0` = Monday .. `6` = Sunday). Rotating shift
/// patterns, a five-day office week and a weekend-only gamer are all just different data.
/// `start_min`/`end_min` bound the active window on those days; `nap` removes a daily
/// break; `holidays` are month/day pairs and `weeks_off` are whole no-screen weeks (their
/// frames are redistributed over the other weeks, so the run still writes
/// `weeks x frames_per_week` frames).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rhythm {
    pub patterns: &'static [&'static [u8]],
    /// Earliest minute of the local day screen time starts (for example `8 * 60`).
    pub start_min: i64,
    /// The day's last frame lands before this minute. At most `1439`.
    pub end_min: i64,
    /// Inclusive range of session counts per active day, 1..=8 with `min <= max`.
    pub sessions: (u8, u8),
    /// A daily absence in minutes from local midnight (lunch, school run, nap).
    pub nap: Option<(i64, i64)>,
    /// Days with no screen time, as `(month, day)` pairs (for example `(12, 25)`).
    pub holidays: &'static [(u8, u8)],
    /// Whole weeks without screen time (vacation or illness), 0-based within the span.
    pub weeks_off: &'static [u32],
}

/// A context the persona works in or on: a repository, a course folder, a client's document
/// folder, a photo shoot, a shop's inventory. One context groups the sessions about it, so
/// the JSON export can report it as a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Context {
    /// One to three words, in the persona's language.
    pub name: &'static str,
    /// Absolute Windows folder on the persona's machine, for example
    /// `C:\Users\marie\Documents\...`.
    pub root: &'static str,
    /// Files or documents inside `root`; at least one name per family the persona's apps
    /// need (see `gen::render`), extensions match `Persona::file_types`.
    pub items: &'static [&'static str],
    /// Topic nouns the persona writes and talks about, in their language.
    pub terms: &'static [&'static str],
    /// The short reference the persona files work under (for example `DOS-14`, `Mariage`).
    pub reference: &'static str,
    /// The public or intranet page about this context, if any (a full `https://` URL).
    pub site: Option<&'static str>,
    /// 0-based week ranges (start inclusive, end exclusive) in which the context is live.
    /// Several ranges encode a context that was started, dropped and later resumed.
    pub weeks: &'static [(u32, u32)],
}

/// A web page the persona reads or uses, rendered as a browser screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Site {
    pub title: &'static str,
    /// A full `https://` URL. Public well-known sites and invented ones are both fine;
    /// nothing here is a real account or a real person's page.
    pub url: &'static str,
    /// Visible body lines, in the persona's language.
    pub lines: &'static [&'static str],
}

/// A document that belongs to no context (a letter, a manual, a recipe, a receipt list),
/// shown in a document, PDF or spreadsheet window and in the file manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocText {
    pub name: &'static str,
    /// Absolute folder holding the document.
    pub folder: &'static str,
    pub lines: &'static [&'static str],
}

/// One synthetic person's definition. Everything the generator places is derived from it,
/// and every persona is a **Windows desktop user**: the product is Windows-only
/// (`CLAUDE.md` hard rule), so all captures come from a Windows machine whatever the
/// persona's life, language or country. The variety lives in the other axes (occupation,
/// language, country, timezone, rhythm, tools and content), not in the desktop platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Persona {
    /// Unique kebab-case id (`gen --persona <id>`).
    pub id: &'static str,
    /// One English line for `gen --list-personas`.
    pub description: &'static str,
    /// Invented full name, in the persona's language.
    pub name: &'static str,
    pub occupation: &'static str,
    /// Age band such as `"25-34"`.
    pub age_band: &'static str,
    /// Invented employer, or `None` for no employer (retired, job seeker, freelancer).
    pub employer: Option<&'static str>,
    /// BCP 47 language tag whose base language must match `labels.lang`.
    pub language: &'static str,
    /// ISO 3166-1 alpha-2 country code.
    pub country: &'static str,
    /// Fixed UTC offset such as `"+02:00"` (the fixture never models DST).
    pub timezone: &'static str,
    /// One or two displays.
    pub monitors: &'static [Monitor],
    /// Localized window chrome words for the persona's language.
    pub labels: &'static Labels,
    pub rhythm: Rhythm,
    /// The persona's applications. At least one per activity kind in `activities`.
    pub apps: &'static [AppDef],
    /// Weighted activities: an activity opens an app of that kind. A persona without
    /// `Code`, `Terminal`, `Files` or a code-host site is a non-technical life.
    pub activities: &'static [(AppKind, u64)],
    pub contexts: &'static [Context],
    pub sites: &'static [Site],
    pub docs: &'static [DocText],
    /// Typical folders for the file manager, absolute Windows paths.
    pub folders: &'static [&'static str],
    /// File extensions the persona meets (`xlsx`, `pdf`, `md`, ...).
    pub file_types: &'static [&'static str],
    /// Topic nouns used in prose, search queries and messages.
    pub topics: &'static [&'static str],
    /// What the persona searches the web for, in their language.
    pub searches: &'static [&'static str],
    /// What the persona watches, listens to or plays.
    pub media: &'static [&'static str],
    /// Invented colleagues, friends and family.
    pub people: &'static [&'static str],
    /// Chat channel names the persona belongs to.
    pub channels: &'static [&'static str],
    /// Correspondence address; every address uses the reserved `example` domain.
    pub email: &'static str,
}

impl Persona {
    /// The identity written to [`PERSONA_SETTING`] for the `export` command.
    pub(crate) fn metadata(&self) -> PersonaMetadata {
        PersonaMetadata {
            id: self.id.to_string(),
            description: self.description.to_string(),
            language: self.language.to_string(),
            country: self.country.to_string(),
            timezone: self.timezone.to_string(),
            // The product is Windows-only (CLAUDE.md hard rule): every fixture capture,
            // whatever the persona's life, is a Windows desktop session.
            os: "windows".to_string(),
        }
    }

    /// The persona's fixed offset, parsed from `timezone`.
    pub(crate) fn offset(&self) -> Result<FixedOffset> {
        offset_of(self.timezone).with_context(|| format!("persona `{}`: invalid timezone", self.id))
    }

    /// The apps of `kind` that the persona actually has, in definition order.
    pub(crate) fn apps_of(&self, kind: AppKind) -> impl Iterator<Item = (usize, &'static AppDef)> {
        self.apps
            .iter()
            .enumerate()
            .filter(move |(_, app)| app.kind == kind)
    }

    /// Whether the persona has at least one app of `kind`.
    pub(crate) fn has_kind(&self, kind: AppKind) -> bool {
        self.apps.iter().any(|app| app.kind == kind)
    }

    /// Whether the context is live in `week` (0-based week of the span).
    pub(crate) fn context_live(context: &Context, week: u32) -> bool {
        context
            .weeks
            .iter()
            .any(|(from, to)| week >= *from && week < *to)
    }
}

/// One persona's public identity, persisted as JSON under [`PERSONA_SETTING`] and reported in
/// the JSON export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PersonaMetadata {
    pub id: String,
    pub description: String,
    pub language: String,
    pub country: String,
    pub timezone: String,
    pub os: String,
}

/// The pinned default fixture's identity. The default `gen` run predates personas and writes
/// no [`PERSONA_SETTING`], so `export` reports this descriptor for it.
pub(crate) fn legacy_metadata() -> PersonaMetadata {
    PersonaMetadata {
        id: "northwind-tech".to_string(),
        description:
            "Default fixture: office software engineer at Northwind Labs (Windows, English, UTC+02:00)"
                .to_string(),
        language: "en-US".to_string(),
        country: "FR".to_string(),
        timezone: "+02:00".to_string(),
        os: "windows".to_string(),
    }
}

/// Parses `"+HH:MM"`/`"-HH:MM"` into a fixed offset.
pub(crate) fn offset_of(timezone: &str) -> Result<FixedOffset> {
    let bytes = timezone.as_bytes();
    ensure!(
        bytes.len() == 6 && (bytes[0] == b'+' || bytes[0] == b'-') && bytes[3] == b':',
        "timezone must look like \"+02:00\", got `{timezone}`"
    );
    let sign = if bytes[0] == b'-' { -1 } else { 1 };
    let hours: i32 = timezone[1..3]
        .parse()
        .with_context(|| format!("timezone hours in `{timezone}`"))?;
    let minutes: i32 = timezone[4..6]
        .parse()
        .with_context(|| format!("timezone minutes in `{timezone}`"))?;
    ensure!(
        hours <= 14 && minutes < 60,
        "timezone `{timezone}` is out of range"
    );
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
        .with_context(|| format!("timezone `{timezone}` is not a valid offset"))
}

/// Resolves the persona identity persisted in a fixture database. A database without the
/// setting is the pinned default fixture and gets [`legacy_metadata`].
pub(crate) fn metadata_for_setting(raw: Option<&str>) -> Result<PersonaMetadata> {
    match raw {
        None => Ok(legacy_metadata()),
        Some(json) => serde_json::from_str(json)
            .with_context(|| format!("persona setting `{PERSONA_SETTING}` is not valid JSON")),
    }
}

/// Every catalog slice, in registry order.
fn sets() -> [&'static [Persona]; 3] {
    [set_a::PERSONAS, set_b::PERSONAS, set_c::PERSONAS]
}

/// The whole persona catalog, in registry order (set A, then B, then C).
pub(crate) fn catalog() -> Vec<&'static Persona> {
    let mut all = Vec::new();
    for set in sets() {
        all.extend(set.iter());
    }
    all
}

/// Looks a persona up by id.
pub(crate) fn find(id: &str) -> Option<&'static Persona> {
    catalog().into_iter().find(|persona| persona.id == id)
}

/// `--persona random`: one persona picked from the seed, independent of every other stream.
pub(crate) fn random(seed: u64) -> &'static Persona {
    let all = catalog();
    debug_assert!(!all.is_empty(), "the persona catalog is never empty");
    all[super::prng::SplitMix64::new(super::prng::subseed(seed, "persona")).below(all.len())]
}

/// `--list-personas`: validates the catalog, then returns one `id<TAB>description` line per
/// persona (trailing newline included).
pub(crate) fn list_lines() -> Result<String> {
    validate_all()?;
    let mut out = String::new();
    for persona in catalog() {
        out.push_str(persona.id);
        out.push('\t');
        out.push_str(persona.description);
        out.push('\n');
    }
    ensure!(!out.is_empty(), "the persona catalog is empty");
    Ok(out)
}

/// Structural validation of the whole catalog: unique ids and every definition well-formed.
pub(crate) fn validate_all() -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for persona in catalog() {
        ensure!(
            seen.insert(persona.id),
            "persona id `{}` is defined twice",
            persona.id
        );
        validate(persona)?;
    }
    ensure!(!seen.is_empty(), "the persona catalog is empty");
    Ok(())
}

fn abs_path(path: &str) -> bool {
    path.starts_with('/')
        || (path.len() > 2
            && path.as_bytes()[1] == b':'
            && matches!(path.as_bytes()[2], b'\\' | b'/'))
}

/// Structural validation of one persona. Messages name the persona and the offending field.
pub(crate) fn validate(persona: &Persona) -> Result<()> {
    let id = persona.id;
    let fail = |what: &str| -> anyhow::Error { anyhow::anyhow!("persona `{id}`: {what}") };
    ensure!(
        !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "persona `{id}`: id must be kebab-case ascii"
    );
    for (field, value) in [
        ("description", persona.description),
        ("name", persona.name),
        ("occupation", persona.occupation),
        ("age_band", persona.age_band),
        ("language", persona.language),
        ("country", persona.country),
        ("timezone", persona.timezone),
        ("email", persona.email),
    ] {
        ensure!(
            !value.trim().is_empty(),
            "{}",
            fail(&format!("empty {field}"))
        );
    }
    ensure!(
        persona.language.split('-').next() == Some(persona.labels.lang),
        "{}",
        fail(&format!(
            "language `{}` does not match labels pack `{}`",
            persona.language, persona.labels.lang
        ))
    );
    ensure!(
        !persona.labels.name.trim().is_empty(),
        "{}",
        fail("labels pack has no display name")
    );
    ensure!(
        persona.country.len() == 2 && persona.country.bytes().all(|b| b.is_ascii_uppercase()),
        "{}",
        fail("country must be an ISO alpha-2 code")
    );
    persona.offset()?;
    ensure!(
        persona
            .employer
            .is_none_or(|employer| !employer.trim().is_empty()),
        "{}",
        fail("employer is empty")
    );
    ensure!(
        persona.email.contains("@example."),
        "{}",
        fail("email must use the reserved example domain")
    );
    ensure!(
        (1..=2).contains(&persona.monitors.len()),
        "{}",
        fail("monitors must hold one or two displays")
    );
    for monitor in persona.monitors {
        ensure!(
            (640..=7680).contains(&monitor.width) && (480..=4320).contains(&monitor.height),
            "{}",
            fail("monitor size is out of range")
        );
    }

    // Rhythm.
    ensure!(
        (1..=4).contains(&persona.rhythm.patterns.len()),
        "{}",
        fail("rhythm.patterns must hold one to four weeks")
    );
    for pattern in persona.rhythm.patterns {
        ensure!(
            (1..=7).contains(&pattern.len()),
            "{}",
            fail("every rhythm week pattern needs one to seven days")
        );
        let mut days = [false; 7];
        for day in *pattern {
            ensure!(*day < 7, "{}", fail("weekday indexes are 0..=6"));
            ensure!(!days[*day as usize], "{}", fail("a weekday repeats"));
            days[*day as usize] = true;
        }
    }
    let rhythm = persona.rhythm;
    ensure!(
        rhythm.start_min >= 0
            && rhythm.end_min > rhythm.start_min
            && rhythm.end_min <= 1439
            && rhythm.end_min - rhythm.start_min >= 90,
        "{}",
        fail("rhythm start_min/end_min must form a 90+ minute window inside one day")
    );
    ensure!(
        (1..=8).contains(&rhythm.sessions.0)
            && rhythm.sessions.0 <= rhythm.sessions.1
            && rhythm.sessions.1 <= 8,
        "{}",
        fail("rhythm.sessions must be 1..=8 with min <= max")
    );
    if let Some((from, to)) = rhythm.nap {
        ensure!(
            from >= rhythm.start_min && to <= rhythm.end_min && from + 10 <= to,
            "{}",
            fail("rhythm.nap must be a 10+ minute window inside the active day")
        );
    }
    for (month, day) in rhythm.holidays {
        ensure!(
            (1..=12).contains(month) && (1..=31).contains(day),
            "{}",
            fail("rhythm.holidays must be (month, day) pairs")
        );
    }

    // Tools.
    ensure!(!persona.apps.is_empty(), "{}", fail("apps is empty"));
    let mut hints = std::collections::HashSet::new();
    for app in persona.apps {
        ensure!(
            !app.name.trim().is_empty() && !app.hint.trim().is_empty(),
            "{}",
            fail("app name/hint is empty")
        );
        ensure!(
            hints.insert(app.hint),
            "{}",
            fail(&format!("app hint `{}` is defined twice", app.hint))
        );
        ensure!(app.weight > 0, "{}", fail("app weight must be positive"));
    }
    ensure!(
        !persona.activities.is_empty(),
        "{}",
        fail("activities is empty")
    );
    ensure!(
        persona.activities.iter().all(|(_, weight)| *weight > 0),
        "{}",
        fail("activity weights must be positive")
    );
    for (kind, _) in persona.activities {
        ensure!(
            persona.has_kind(*kind),
            "{}",
            fail(&format!(
                "no app of kind `{}` for the activity",
                kind.as_str()
            ))
        );
    }

    // Environment/content.
    ensure!(
        (1..=6).contains(&persona.contexts.len()),
        "{}",
        fail("contexts must hold one to six entries")
    );
    for context in persona.contexts {
        ensure!(
            !context.name.trim().is_empty() && !context.reference.trim().is_empty(),
            "{}",
            fail("context name/reference is empty")
        );
        ensure!(
            abs_path(context.root),
            "{}",
            fail(&format!(
                "context `{}` root is not an absolute path",
                context.name
            ))
        );
        ensure!(
            (1..=10).contains(&context.items.len()),
            "{}",
            fail(&format!(
                "context `{}` needs one to ten items",
                context.name
            ))
        );
        ensure!(
            context.items.iter().all(|item| !item.trim().is_empty()),
            "{}",
            fail(&format!("context `{}` has an empty item", context.name))
        );
        ensure!(
            (3..=8).contains(&context.terms.len()),
            "{}",
            fail(&format!(
                "context `{}` needs three to eight terms",
                context.name
            ))
        );
        ensure!(
            context.terms.iter().all(|term| !term.trim().is_empty()),
            "{}",
            fail(&format!("context `{}` has an empty term", context.name))
        );
        if let Some(site) = context.site {
            ensure!(
                site.starts_with("https://"),
                "{}",
                fail(&format!("context `{}` site must be https", context.name))
            );
        }
        ensure!(
            !context.weeks.is_empty(),
            "{}",
            fail(&format!("context `{}` needs a week range", context.name))
        );
        let mut previous = None;
        for (from, to) in context.weeks {
            ensure!(
                from < to,
                "{}",
                fail(&format!(
                    "context `{}` has an empty week range",
                    context.name
                ))
            );
            if let Some(previous) = previous {
                ensure!(
                    from >= previous,
                    "{}",
                    fail(&format!(
                        "context `{}` week ranges must be sorted and disjoint",
                        context.name
                    ))
                );
            }
            previous = Some(to);
        }
    }
    ensure!(
        (3..=8).contains(&persona.sites.len()),
        "{}",
        fail("sites must hold three to eight entries")
    );
    for site in persona.sites {
        ensure!(
            !site.title.trim().is_empty() && site.url.starts_with("https://"),
            "{}",
            fail("site needs a title and an https URL")
        );
        ensure!(
            (1..=8).contains(&site.lines.len()) && site.lines.iter().all(|line| !line.is_empty()),
            "{}",
            fail(&format!(
                "site `{}` needs one to eight non-empty lines",
                site.title
            ))
        );
    }
    ensure!(
        (1..=4).contains(&persona.docs.len()),
        "{}",
        fail("docs must hold one to four entries")
    );
    for doc in persona.docs {
        ensure!(
            doc.name.contains('.') && !doc.name.trim().is_empty(),
            "{}",
            fail(&format!(
                "doc `{}` needs a file name with an extension",
                doc.name
            ))
        );
        ensure!(
            abs_path(doc.folder),
            "{}",
            fail(&format!("doc `{}` folder is not absolute", doc.name))
        );
        ensure!(
            (1..=6).contains(&doc.lines.len()) && doc.lines.iter().all(|line| !line.is_empty()),
            "{}",
            fail(&format!(
                "doc `{}` needs one to six non-empty lines",
                doc.name
            ))
        );
    }
    ensure!(
        (2..=8).contains(&persona.folders.len()),
        "{}",
        fail("folders must hold two to eight entries")
    );
    for folder in persona.folders {
        ensure!(
            abs_path(folder),
            "{}",
            fail("every folder must be an absolute path")
        );
    }
    ensure!(
        (2..=6).contains(&persona.file_types.len()),
        "{}",
        fail("file_types must hold two to six extensions")
    );
    for extension in persona.file_types {
        ensure!(
            !extension.is_empty()
                && !extension.starts_with('.')
                && extension
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "{}",
            fail(&format!(
                "file type `{extension}` must be lowercase alphanumeric, without a dot"
            ))
        );
    }
    for (field, values, range) in [
        ("topics", persona.topics, 4..=12),
        ("searches", persona.searches, 3..=10),
        ("media", persona.media, 2..=10),
        ("people", persona.people, 3..=10),
        ("channels", persona.channels, 2..=6),
    ] {
        ensure!(
            range.contains(&values.len()),
            "{}",
            fail(&format!("{field} must hold the required number of entries"))
        );
        ensure!(
            values.iter().all(|value| !value.trim().is_empty()),
            "{}",
            fail(&format!("{field} has an empty entry"))
        );
    }
    Ok(())
}
