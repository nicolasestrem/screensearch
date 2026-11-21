//! Generic persona screen rendering.
//!
//! One renderer family per [`AppKind`], never per persona: a spreadsheet screen is drawn from
//! the persona's [`Context`](super::personas::Context) items and terms whatever the persona's
//! occupation is, and the language comes from the persona's label pack. Window titles, URLs,
//! `app_hint` values and body text always agree, exactly like the default fixture's screens:
//! a browser frame has a URL and shows it in the omnibox, an editor frame names the file, a
//! terminal frame shows its working directory, an office frame names its document.
//!
//! Every line is synthetic, in the persona's language, and carries no banking, medical or
//! intimate content: the context terms and documents are invented phrases about the work.

use super::layout::Screen;
use super::life::{local, LifeFrame, LifeSession};
use super::personas::{AppDef, AppKind, Context, Labels, Persona};
use super::prng::{subseed, SplitMix64};

/// The browser page a session kept on screen between frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Site(usize),
    Search(usize),
    Context(usize),
}

/// What a session kept on screen between its frames.
#[derive(Debug, Default)]
pub(crate) struct Sticky {
    page: Option<Page>,
}

/// Everything a renderer needs about the frame being drawn.
struct Ctx {
    app: &'static AppDef,
    context: Option<(&'static Context, usize)>,
    session_at: i64,
}

/// One persona's seeded content drawer.
pub(crate) struct Renderer {
    persona: &'static Persona,
    rng: SplitMix64,
    offset: chrono::FixedOffset,
}

impl Renderer {
    pub(crate) fn new(seed: u64, persona: &'static Persona) -> Self {
        Self {
            persona,
            rng: SplitMix64::new(subseed(seed, "persona-render")),
            offset: persona.offset().expect("validated persona has an offset"),
        }
    }

    fn labels(&self) -> &'static Labels {
        self.persona.labels
    }

    fn below(&mut self, n: usize) -> usize {
        self.rng.below(n)
    }

    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.rng.chance(num, den)
    }

    fn person(&mut self) -> &'static str {
        let count = self.persona.people.len();
        let index = self.below(count);
        self.persona.people[index]
    }

    fn term(&mut self, c: &Ctx) -> String {
        match c.context {
            Some((context, _)) => {
                let count = context.terms.len();
                context.terms[self.below(count)].to_string()
            }
            None => pick(&mut self.rng, self.persona.topics).to_string(),
        }
    }

    fn item(&mut self, c: &Ctx) -> String {
        match c.context {
            Some((context, _)) => {
                let count = context.items.len();
                context.items[self.below(count)].to_string()
            }
            None => pick(&mut self.rng, self.persona.docs).name.to_string(),
        }
    }

    /// A document name from the context or the persona's own documents, filtered by
    /// extension when `exts` is non-empty, with its body lines.
    fn document_data(&mut self, c: &Ctx, exts: &[&str]) -> (String, Vec<String>) {
        let matches = |name: &str| exts.is_empty() || exts.contains(&ext_of(name));
        let mut candidates: Vec<String> = Vec::new();
        if let Some((context, _)) = c.context {
            candidates.extend(
                context
                    .items
                    .iter()
                    .filter(|name| matches(name))
                    .map(|name| name.to_string()),
            );
        }
        candidates.extend(
            self.persona
                .docs
                .iter()
                .filter(|doc| matches(doc.name))
                .map(|doc| doc.name.to_string()),
        );
        if candidates.is_empty() {
            candidates.push(self.item(c));
        }
        let pick = self.below(candidates.len());
        let name = candidates[pick].clone();
        let known = self
            .persona
            .docs
            .iter()
            .find(|doc| doc.name == name)
            .map(|doc| doc.lines.iter().map(|line| line.to_string()).collect());
        let lines = match known {
            Some(lines) => lines,
            None => self.document_lines(c, &name),
        };
        (name, lines)
    }

    /// Body prose for a document whose text is not part of the persona's `docs`.
    fn document_lines(&mut self, c: &Ctx, name: &str) -> Vec<String> {
        let labels = self.labels();
        let mut lines = vec![humanize(name)];
        if let Some((context, _)) = c.context {
            lines.push(format!("{}: {}", context.reference, context.name));
            for _ in 0..2 {
                let term_count = context.terms.len();
                let item_count = context.items.len();
                let page = 1 + self.below(4);
                let of = 4 + self.below(14);
                lines.push(format!(
                    "{} {} {}",
                    context.terms[self.below(term_count)],
                    pick(&mut self.rng, self.persona.topics),
                    context.items[self.below(item_count)]
                ));
                lines.push(format!("{} {} {} {}", labels.page, page, labels.of, of));
            }
        } else {
            let doc = pick(&mut self.rng, self.persona.docs);
            let line_count = doc.lines.len();
            lines.push(doc.lines[self.below(line_count)].to_string());
            lines.push(format!("{}: {}", labels.settings, self.persona.occupation));
        }
        lines
    }

    /// A browser page: a saved site, a search or the context's own page.
    fn choose_page(&mut self, c: &Ctx, sticky: &mut Sticky) -> Page {
        if let Some(page) = sticky.page {
            if self.chance(1, 2) {
                return page;
            }
        }
        let roll = self.below(100);
        let site_count = self.persona.sites.len();
        let search_count = self.persona.searches.len();
        let page = if roll < 55 || site_count == 0 {
            Page::Site(self.below(site_count))
        } else if roll < 80 && search_count > 0 {
            Page::Search(self.below(search_count))
        } else if let Some((_, index)) = c.context {
            Page::Context(index)
        } else {
            Page::Site(self.below(site_count))
        };
        sticky.page = Some(page);
        page
    }

    fn page(&mut self, c: &Ctx, page: Page) -> (String, String, Vec<String>) {
        let labels = self.labels();
        match page {
            Page::Site(index) => {
                let site = &self.persona.sites[index];
                let mut body: Vec<String> =
                    site.lines.iter().map(|line| line.to_string()).collect();
                if let Some((context, _)) = c.context {
                    body.push(format!("{}: {}", labels.search, context.reference));
                }
                (site.title.to_string(), site.url.to_string(), body)
            }
            Page::Search(index) => {
                let query = self.persona.searches[index];
                let mut body = vec![format!("{} \"{}\"", labels.search, query)];
                let mut used: Vec<usize> = Vec::new();
                for _ in 0..3 {
                    let count = self.persona.sites.len();
                    let pick = self.below(count);
                    if used.contains(&pick) {
                        continue;
                    }
                    used.push(pick);
                    let site = &self.persona.sites[pick];
                    let line = site.lines[self.below(site.lines.len())];
                    body.push(format!("{} - {}", site.title, line));
                }
                if used.is_empty() {
                    body.push(self.term(c));
                }
                let url = format!("https://search.example/?q={}", url_encode(query));
                (format!("{} - {}", query, labels.search), url, body)
            }
            Page::Context(index) => {
                let context = &self.persona.contexts[index];
                let url = match context.site {
                    Some(site) => site.to_string(),
                    None => {
                        let base = self.persona.sites[0].url;
                        let host = caller_host(base);
                        format!("https://{host}/{}", url_encode(context.name))
                    }
                };
                let mut body = vec![
                    format!("{}: {} {}", labels.search, context.reference, context.name),
                    format!("{} {}", context.terms.join(" · "), context.name),
                ];
                for item in context.items.iter().take(3) {
                    body.push(format!("{} - {}", humanize(item), context.terms[0]));
                }
                (context.name.to_string(), url, body)
            }
        }
    }

    /// The window chrome every office-style app shares.
    fn menu(&self) -> Vec<String> {
        let labels = self.labels();
        vec![format!(
            "{} {} {} {} {} {} {}",
            labels.file,
            labels.edit,
            labels.view,
            labels.insert,
            labels.format,
            labels.tools,
            labels.help
        )]
    }

    fn background(&mut self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .persona
            .sites
            .iter()
            .take(3)
            .map(|site| site.title.to_string())
            .collect();
        lines.extend(
            self.persona
                .channels
                .iter()
                .map(|channel| format!("# {channel}")),
        );
        lines.extend(
            self.persona
                .media
                .iter()
                .take(2)
                .map(|title| title.to_string()),
        );
        lines
    }

    fn file_row(&mut self, name: &str, at: i64) -> String {
        let labels = self.labels();
        let days = 1 + self.below(27) as i64;
        let date = local(at - days * 86_400_000, self.offset)
            .format(labels.date_format)
            .to_string();
        let size = self.below(900) + 4;
        format!("{name}  {date}  {size} KB")
    }

    /// A file inside a context folder, in Windows spelling.
    fn path(&self, root: &str, item: &str) -> String {
        format!("{root}\\{item}")
    }

    // ----------------------------------------------------------------------------------
    // Apps
    // ----------------------------------------------------------------------------------

    fn browser(&mut self, c: &Ctx, sticky: &mut Sticky) -> Screen {
        let app = c.app;
        let page = self.choose_page(c, sticky);
        let (page_title, url, body) = self.page(c, page);
        let bookmarks: Vec<String> = self
            .persona
            .sites
            .iter()
            .take(3)
            .map(|site| site.title.to_string())
            .collect();
        let windowed = self.chance(1, 6);
        let background = if windowed {
            self.background()
        } else {
            Vec::new()
        };
        Screen {
            app_hint: app.hint,
            title: format!("{} - {}", page_title, app.name),
            url: Some(url.clone()),
            title_bar: false,
            top: vec![truncate_chars(&page_title, 34), url, bookmarks.join("  ")],
            side: Vec::new(),
            body,
            bottom: Vec::new(),
            windowed,
            background,
        }
    }

    fn mail(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let sender = self.person();
        let subject = match c.context {
            Some((context, _)) => format!("{}: {}", context.reference, humanize(&self.item(c))),
            None => format!("{} {}", labels.settings, self.persona.occupation),
        };
        let mut body = Vec::new();
        let mut minutes_ago = 0i64;
        for row in 0..3 {
            minutes_ago += 7 + self.below(40) as i64;
            let who = self.person();
            let row_subject = match c.context {
                Some((context, _)) if row == 0 => {
                    format!("{} {}", context.reference, context.terms[0])
                }
                Some((context, _)) => {
                    let index = row % context.items.len();
                    humanize(context.items[index])
                }
                None => {
                    let index = row % self.persona.docs.len();
                    humanize(self.persona.docs[index].name)
                }
            };
            let when = local(c.session_at - minutes_ago * 60_000, self.offset)
                .format(labels.time_format)
                .to_string();
            body.push(format!("{who}  {row_subject}  {when}"));
        }
        body.push(subject);
        body.push(format!("{sender} <{}>", email_of(sender)));
        body.push(format!(
            "To: {} <{}>",
            self.persona.name, self.persona.email
        ));
        for line in 0..3 {
            let line = match line {
                0 => format!("{} {}, {}", labels.view, self.term(c), self.item(c)),
                1 => format!("{}: {}", self.term(c), humanize(&self.item(c))),
                _ => {
                    let topic_count = self.persona.topics.len();
                    let index = self.below(topic_count);
                    let topic = self.persona.topics[index];
                    format!("{} {}", topic, self.term(c))
                }
            };
            body.push(line);
        }
        Screen {
            app_hint: app.hint,
            title: format!("{} - {} - {}", labels.inbox, self.persona.email, app.name),
            title_bar: true,
            top: vec![
                self.menu().remove(0),
                format!(
                    "{} {} {} {}",
                    labels.inbox, labels.sent, labels.drafts, labels.archive
                ),
            ],
            side: [
                labels.inbox,
                labels.sent,
                labels.drafts,
                labels.archive,
                labels.settings,
            ]
            .iter()
            .map(|label| label.to_string())
            .collect(),
            body,
            bottom: vec![
                format!("{}: {}", labels.items, 12 + self.below(60)),
                labels.search.to_string(),
            ],
            ..Screen::default()
        }
    }

    fn spreadsheet(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let (name, mut body) = self.document_data(c, &["xlsx", "csv", "ods", "numbers"]);
        body.insert(0, "A B C D E F".to_string());
        if let Some((context, _)) = c.context {
            body.push(format!(
                "{} {} {} {}",
                context.terms[0], context.terms[1], context.terms[2], context.reference
            ));
            for row in 0..4 + self.below(4) {
                body.push(format!(
                    "{}-{} {} {} {}",
                    context.reference,
                    row + 1,
                    10 + self.below(90),
                    self.below(1000),
                    context.terms[row % context.terms.len()]
                ));
            }
        }
        Screen {
            app_hint: app.hint,
            title: format!("{} - {}", name, app.name),
            title_bar: true,
            top: self.menu(),
            side: Vec::new(),
            body,
            bottom: vec![
                labels.save.to_string(),
                format!("{} {} {} {}", labels.page, 1 + self.below(4), labels.of, 4),
            ],
            ..Screen::default()
        }
    }

    fn word_processor(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let (name, body) = self.document_data(c, &["docx", "md", "odt", "txt", "pages", "rtf"]);
        let pages = 2 + self.below(9);
        Screen {
            app_hint: app.hint,
            title: format!("{} - {}", name, app.name),
            title_bar: true,
            top: self.menu(),
            side: Vec::new(),
            body,
            bottom: vec![format!(
                "{} {} {} {} · {} {}",
                labels.page,
                1 + self.below(pages),
                labels.of,
                pages,
                300 + self.below(2400),
                labels.items
            )],
            ..Screen::default()
        }
    }

    fn slides(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let (name, mut body) = self.document_data(c, &["pptx", "odp", "key"]);
        body.insert(0, format!("{} - {}", humanize(&name), labels.slide));
        if let Some((context, _)) = c.context {
            for item in context.items.iter().take(3) {
                body.push(format!("- {} ({})", humanize(item), context.terms[0]));
            }
        }
        let slides = 6 + self.below(12);
        Screen {
            app_hint: app.hint,
            title: format!("{} - {}", name, app.name),
            title_bar: true,
            top: self.menu(),
            side: (1..=6).map(|n| n.to_string()).collect(),
            body,
            bottom: vec![format!(
                "{} {} {} {}",
                labels.slide,
                1 + self.below(slides),
                labels.of,
                slides
            )],
            ..Screen::default()
        }
    }

    fn pdf(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let (name, body) = self.document_data(c, &["pdf"]);
        let pages = 3 + self.below(18);
        Screen {
            app_hint: app.hint,
            title: format!("{} - {}", name, app.name),
            title_bar: true,
            top: vec![format!("{} {} {}", labels.view, labels.tools, labels.help)],
            side: Vec::new(),
            body,
            bottom: vec![format!(
                "{} {} {} {}",
                labels.page,
                1 + self.below(pages),
                labels.of,
                pages
            )],
            ..Screen::default()
        }
    }

    fn chat(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let channel = match c.context {
            Some((context, _)) => context.name.to_string(),
            None => pick(&mut self.rng, self.persona.channels).to_string(),
        };
        let workspace = self.persona.employer.unwrap_or(self.persona.occupation);
        let mut body = vec![format!("# {channel}")];
        let messages = 3 + self.below(3);
        let mut minutes_ago = 0i64;
        for _ in 0..messages {
            minutes_ago += 2 + self.below(9) as i64;
            let author = if self.chance(1, 3) {
                self.persona.name
            } else {
                self.person()
            };
            let when = local(c.session_at - minutes_ago * 60_000, self.offset)
                .format(labels.time_format)
                .to_string();
            body.push(format!("{author}  {when}"));
            let line = match self.below(5) {
                0 => {
                    let item = humanize(&self.item(c));
                    format!("{item}: {}", self.term(c))
                }
                1 => {
                    let term = self.term(c);
                    format!("{term} -> {}", self.item(c))
                }
                2 => {
                    let topic_count = self.persona.topics.len();
                    let index = self.below(topic_count);
                    let topic = self.persona.topics[index];
                    format!("{}, {}", self.term(c), topic)
                }
                3 => match c.context {
                    Some((context, _)) => format!("{} {}", context.reference, context.terms[0]),
                    None => self.term(c),
                },
                _ => {
                    let reference = c
                        .context
                        .map(|(context, _)| context.reference.to_string())
                        .unwrap_or_else(|| self.persona.channels[0].to_string());
                    format!("{} {}", self.term(c), reference)
                }
            };
            body.push(line);
        }
        let mut side: Vec<String> = [labels.home, labels.search]
            .iter()
            .map(|label| label.to_string())
            .collect();
        side.extend(self.persona.channels.iter().map(|name| format!("# {name}")));
        side.extend(
            self.persona
                .people
                .iter()
                .take(4)
                .map(|name| name.to_string()),
        );
        let windowed = self.chance(1, 4);
        let background = if windowed {
            self.background()
        } else {
            Vec::new()
        };
        Screen {
            app_hint: app.hint,
            title: format!("{channel} - {workspace} - {}", app.name),
            title_bar: false,
            top: vec![labels.search.to_string()],
            side,
            body,
            bottom: vec![format!("{} #{}", labels.new, channel.to_lowercase())],
            windowed,
            background,
            ..Screen::default()
        }
    }

    fn files(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let folder_count = self.persona.folders.len();
        let folder_min: &'static [&'static str] = self.persona.folders;
        let folder = match c.context {
            Some((context, _)) if self.chance(1, 2) => context.root.to_string(),
            _ => folder_min[self.below(folder_count)].to_string(),
        };
        let at = c.session_at;
        let mut rows: Vec<String> = Vec::new();
        if let Some((context, _)) = c.context {
            if folder == context.root {
                for item in context.items {
                    rows.push(self.file_row(item, at));
                }
            }
        }
        for doc in self.persona.docs {
            if doc.folder == folder && !rows.iter().any(|row| row.starts_with(doc.name)) {
                rows.push(self.file_row(doc.name, at));
            }
        }
        while rows.len() < 3 {
            let term = pick(&mut self.rng, self.persona.topics);
            let extension = pick(&mut self.rng, self.persona.file_types);
            let name = format!("{term}-2026.{extension}");
            if !rows.iter().any(|row| row.starts_with(&name)) {
                rows.push(self.file_row(&name, at));
            }
        }
        let shown = rows.len();
        let mut body = vec![folder.clone()];
        body.extend(rows);
        Screen {
            app_hint: app.hint,
            title: format!("{folder} - {}", app.name),
            title_bar: true,
            top: vec![format!(
                "{} {} {} {} {} {}",
                labels.new, labels.open, labels.save, labels.print, labels.view, labels.settings
            )],
            side: [
                labels.home,
                labels.documents,
                labels.downloads,
                labels.desktop,
                labels.pictures,
                labels.music,
            ]
            .iter()
            .map(|label| label.to_string())
            .collect(),
            body,
            bottom: vec![format!("{shown} {}", labels.items)],
            windowed: self.chance(1, 4),
            background: Vec::new(),
            ..Screen::default()
        }
    }

    fn meeting(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let title = match c.context {
            Some((context, _)) => context.name.to_string(),
            None => format!("{} {}", labels.settings, self.persona.occupation),
        };
        let mut body = vec![title.clone()];
        for _ in 0..3 {
            body.push(self.person().to_string());
        }
        body.push(self.persona.name.to_string());
        let peer = self.person();
        body.push(format!("{}: {peer}", labels.view));
        if let Some((context, _)) = c.context {
            body.push(format!("{}: {}", labels.open, humanize(&self.item(c))));
            body.push(format!("{} {}", context.reference, context.terms[0]));
        }
        let windowed = self.chance(1, 3);
        let background = if windowed {
            self.background()
        } else {
            Vec::new()
        };
        Screen {
            app_hint: app.hint,
            title: format!("{title} - {}", app.name),
            title_bar: false,
            top: vec![labels.view.to_string()],
            side: Vec::new(),
            body,
            bottom: Vec::new(),
            windowed,
            background,
            ..Screen::default()
        }
    }

    fn notes(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let topic = pick(&mut self.rng, self.persona.topics);
        let page_title = match c.context {
            Some((context, _)) => format!("{} · {topic}", context.name),
            None => format!("{} · {topic}", self.persona.occupation),
        };
        let mut body = vec![format!("{}: {topic}", labels.settings)];
        for _ in 0..3 {
            body.push(format!("- {}: {}", self.term(c), self.item(c)));
        }
        let peer = self.person();
        body.push(format!("{}: {peer}", labels.view));
        let mut side = vec![labels.search.to_string(), labels.home.to_string()];
        side.extend(
            self.persona
                .channels
                .iter()
                .take(3)
                .map(|name| name.to_string()),
        );
        let windowed = self.chance(1, 4);
        let background = if windowed {
            self.background()
        } else {
            Vec::new()
        };
        Screen {
            app_hint: app.hint,
            title: format!("{page_title} - {}", app.name),
            title_bar: true,
            top: vec![labels.file.to_string()],
            side,
            body,
            bottom: vec![labels.save.to_string()],
            windowed,
            background,
            ..Screen::default()
        }
    }

    fn canvas(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let item = self.item(c);
        let name = humanize(&item);
        let mut body = vec![format!("{} - {name}", labels.view)];
        for _ in 0..3 {
            let number = self.below(16);
            body.push(format!("{} {number:>3} {}", labels.insert, self.term(c)));
        }
        if let Some((context, _)) = c.context {
            body.push(format!("{}: {}", context.reference, context.terms[0]));
        }
        let mut side: Vec<String> = vec![labels.view.to_string()];
        if let Some((context, _)) = c.context {
            side.extend(context.items.iter().take(4).map(|item| humanize(item)));
        }
        side.extend(
            self.persona
                .media
                .iter()
                .take(2)
                .map(|title| title.to_string()),
        );
        let level = self.below(30) + 70;
        Screen {
            app_hint: app.hint,
            title: format!("{name} - {}", app.name),
            title_bar: true,
            top: self.menu(),
            side,
            body,
            bottom: vec![format!("{}: {level}", labels.save)],
            ..Screen::default()
        }
    }

    fn game(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let title = pick(&mut self.rng, self.persona.media);
        let mut body = vec![title.to_string()];
        for media in self.persona.media {
            body.push(format!("{}  {}", media, labels.view));
        }
        for _ in 0..2 {
            let peer = self.person();
            body.push(format!("{peer} {}", labels.home));
        }
        let mut side = vec![labels.home.to_string(), labels.settings.to_string()];
        side.extend(
            self.persona
                .channels
                .iter()
                .take(2)
                .map(|name| format!("# {name}")),
        );
        Screen {
            app_hint: app.hint,
            title: format!("{title} - {}", app.name),
            title_bar: false,
            top: vec![labels.search.to_string()],
            side,
            body,
            bottom: Vec::new(),
            ..Screen::default()
        }
    }

    fn code(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let (context, _) = c.context.expect("code sessions carry a context");
        let item_count = context.items.len();
        let term_count = context.terms.len();
        let item = context.items[self.below(item_count)];
        let path = self.path(context.root, item);
        let mut body = Vec::new();
        for line in 0..14 {
            let term = context.terms[line % term_count];
            let target = context.items[(line * 3) % item_count];
            let reference = context.reference;
            body.push(format!("{:<3} {term} = {target}  # {reference}", line + 1));
        }
        let mut side = vec![labels.file.to_uppercase()];
        side.extend(context.items.iter().map(|item| item.to_string()));
        Screen {
            app_hint: app.hint,
            title: format!("{path} - {}", app.name),
            title_bar: true,
            top: vec![
                self.menu().remove(0),
                format!("{} {}", labels.save, context.reference),
            ],
            side,
            body,
            bottom: vec![
                context.reference.to_string(),
                format!("{} {} {}", labels.page, 12, labels.of),
            ],
            ..Screen::default()
        }
    }

    fn terminal(&mut self, c: &Ctx) -> Screen {
        let labels = self.labels();
        let app = c.app;
        let (context, _) = c.context.expect("terminal sessions carry a context");
        let prompt = format!("PS {}>", context.root);
        let mut body = vec![format!("{prompt} ls -1")];
        for item in context.items {
            body.push(item.to_string());
        }
        body.push(format!("{prompt} grep -r \"{}\" .", context.terms[0]));
        for item in context.items.iter().take(2) {
            let path = self.path(context.root, item);
            let line = 12 + self.below(40);
            body.push(format!("{path}:{line}:{}", context.terms[0]));
        }
        body.push(format!("{prompt} {}", labels.search));
        Screen {
            app_hint: app.hint,
            title: format!("{} - {}", context.root, app.name),
            title_bar: true,
            top: vec![self.menu().remove(0)],
            side: Vec::new(),
            body,
            bottom: vec![context.reference.to_string()],
            ..Screen::default()
        }
    }
}

/// Draws one frame of one persona session.
pub(crate) fn render(
    renderer: &mut Renderer,
    session: &LifeSession,
    frame: &LifeFrame,
    sticky: &mut Sticky,
) -> Screen {
    let app = &renderer.persona.apps[frame.app];
    let c = Ctx {
        app,
        context: session
            .context
            .map(|index| (&renderer.persona.contexts[index], index)),
        session_at: frame.at,
    };
    match app.kind {
        AppKind::Browser => renderer.browser(&c, sticky),
        AppKind::Mail => renderer.mail(&c),
        AppKind::Spreadsheet => renderer.spreadsheet(&c),
        AppKind::Document => renderer.word_processor(&c),
        AppKind::Slides => renderer.slides(&c),
        AppKind::Pdf => renderer.pdf(&c),
        AppKind::Chat => renderer.chat(&c),
        AppKind::Files => renderer.files(&c),
        AppKind::Meeting => renderer.meeting(&c),
        AppKind::Notes => renderer.notes(&c),
        AppKind::Canvas => renderer.canvas(&c),
        AppKind::Game => renderer.game(&c),
        AppKind::Code => renderer.code(&c),
        AppKind::Terminal => renderer.terminal(&c),
    }
}

/// Uniform pick from a static slice, without borrowing self for the index draw.
fn pick<T>(rng: &mut SplitMix64, items: &'static [T]) -> &'static T {
    &items[rng.below(items.len())]
}

fn ext_of(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

/// `riego-semana.xlsx` -> `Riego semana`.
fn humanize(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let words: Vec<&str> = stem
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .collect();
    let mut out = words.join(" ");
    if let Some(first) = out.get(..1) {
        out = first.to_uppercase() + &out[1..];
    }
    out
}

/// An invented address for an invented person, from their invented name.
fn email_of(person: &str) -> String {
    let local: String = person
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == ' ' || *ch == '.' || *ch == '-')
        .map(|ch| if ch == ' ' { '.' } else { ch })
        .collect();
    format!("{local}@example.com")
}

/// The host of a full URL.
fn caller_host(url: &str) -> &str {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    &rest[..end]
}

/// Percent-encodes everything outside the unrestricted URL alphabet.
fn url_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~');
        if keep {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
