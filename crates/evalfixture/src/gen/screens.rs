//! Per-application screen rendering.
//!
//! Each frame becomes a [`Screen`]: the window title and URL the capture loop would record,
//! and the text the screen shows, split into the edge chrome the attention filter is meant
//! to drop and the body it must keep. Titles, URLs, `app_hint` and body always agree: a
//! browser frame has a URL and shows it in the omnibox (full URLs are enabled), an editor
//! frame has the file's absolute path in its title (`window.title` = `${activeEditorLong}`),
//! a terminal frame shows its working directory, an Office frame names its document.
//!
//! Entities are drawn least-used-first half of the time and uniformly otherwise, so every
//! entity keeps recurring while hot ones recur more; a session keeps its file, page or
//! document across consecutive frames the way real work does.

use std::collections::HashMap;

use super::code::{
    camel, ci_steps, claude_session, file_body, git_log, git_remote, git_status, prompt,
    rg_listing, short_hash, stem_of, teammate, test_run, viewport,
};
use super::layout::Screen;
use super::plan::{admin_folders, local, Activity, Anchor, App, FramePlan, SessionPlan};
use super::prng::{subseed, SplitMix64};
use super::world::{
    AdminDoc, Lang, Project, Ticket, Tracker, ADMIN_DOCS, CI, CLAUDE, GRAFANA, PAGERDUTY, PEOPLE,
    PROJECTS, RESOURCES, SENTRY, USER_EMAIL, USER_NAME,
};

/// A browser page a session can be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Ticket(usize),
    Board,
    CodeHost,
    Ci,
    Sentry,
    Grafana,
    PagerDuty,
    Site(usize),
    Resource(usize),
    Claude,
}

/// An Office document: one of the project's documents or an admin document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doc {
    Project(usize, usize),
    Admin(usize),
}

impl Doc {
    fn name(self) -> &'static str {
        match self {
            Doc::Project(p, d) => PROJECTS[p].docs[d],
            Doc::Admin(i) => ADMIN_DOCS[i].name,
        }
    }
}

/// What a session keeps on screen between its frames.
#[derive(Debug, Default)]
pub(crate) struct Sticky {
    file: Option<usize>,
    scroll: usize,
    page: Option<Page>,
    doc: Option<Doc>,
    ticket: Option<usize>,
    claude_chat: Option<String>,
}

/// Everything a renderer needs about the frame being drawn.
struct Ctx<'a> {
    p: &'static Project,
    pi: usize,
    has_project: bool,
    activity: Activity,
    at: i64,
    echo: Option<&'a str>,
}

/// Seeded content drawer with per-entity usage counts.
pub(crate) struct Renderer {
    rng: SplitMix64,
    usage: HashMap<String, u32>,
    build: u64,
}

fn hm(at: i64) -> String {
    local(at).format("%H:%M").to_string()
}

/// The taskbar clock: `HH:MM` and `DD/MM/YYYY`.
pub(crate) fn clock(at: i64) -> (String, String) {
    let t = local(at);
    (
        t.format("%H:%M").to_string(),
        t.format("%d/%m/%Y").to_string(),
    )
}

fn email_of(person: &str) -> String {
    format!(
        "{}@northwind.example",
        person.to_ascii_lowercase().replace(' ', ".")
    )
}

fn lang_label(lang: Lang, file_name: &str) -> &'static str {
    match file_name.rsplit_once('.').map(|(_, e)| e) {
        Some("md") => "Markdown",
        Some("toml") => "TOML",
        Some("kts") => "Kotlin",
        Some("tfvars") | Some("tf") => "Terraform",
        Some("tsx") => "TypeScript JSX",
        _ => match lang {
            Lang::Rust => "Rust",
            Lang::Python | Lang::Docs => "Python",
            Lang::TypeScript => "TypeScript",
            Lang::Go => "Go",
            Lang::CSharp => "C#",
            Lang::Kotlin => "Kotlin",
            Lang::Terraform => "Terraform",
        },
    }
}

/// Generic text from a calendar window parked to the right of a windowed app; the filter
/// drops it as background-window text.
const CALENDAR_BACKGROUND: [&str; 8] = [
    "Calendar",
    "Today",
    "09:30 Standup",
    "11:00 Design review",
    "13:30 Focus time",
    "15:00 1:1",
    "16:30 Release sync",
    "Week 37 September 2026",
];

impl Renderer {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(subseed(seed, "render")),
            usage: HashMap::new(),
            build: 88_000,
        }
    }

    fn below(&mut self, n: usize) -> usize {
        self.rng.below(n)
    }

    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.rng.chance(num, den)
    }

    /// Least-used-first half of the time (ties broken by the stream), uniform otherwise.
    fn pick(&mut self, keys: &[String]) -> usize {
        let choice = if self.chance(1, 2) {
            let counts: Vec<u32> = keys
                .iter()
                .map(|k| self.usage.get(k).copied().unwrap_or(0))
                .collect();
            let min = counts.iter().copied().min().unwrap_or(0);
            let tied: Vec<usize> = (0..keys.len()).filter(|i| counts[*i] == min).collect();
            tied[self.below(tied.len())]
        } else {
            self.below(keys.len())
        };
        *self.usage.entry(keys[choice].clone()).or_insert(0) += 1;
        choice
    }

    fn pick_file(&mut self, c: &Ctx) -> usize {
        let keys: Vec<String> = (0..4).map(|i| format!("file:{}:{i}", c.p.slug)).collect();
        self.pick(&keys)
    }

    fn pick_ticket(&mut self, c: &Ctx) -> usize {
        let keys: Vec<String> =
            c.p.tickets
                .iter()
                .map(|t| format!("ticket:{}", t.key))
                .collect();
        self.pick(&keys)
    }

    fn pick_doc_index(&mut self, c: &Ctx) -> usize {
        let keys: Vec<String> = c.p.docs.iter().map(|d| format!("doc:{d}")).collect();
        self.pick(&keys)
    }

    fn pick_resource(&mut self, lang: Option<Lang>) -> usize {
        let mut candidates: Vec<usize> = (0..RESOURCES.len())
            .filter(|i| {
                lang.is_none() || RESOURCES[*i].lang.is_none() || RESOURCES[*i].lang == lang
            })
            .collect();
        if candidates.is_empty() {
            candidates = (0..RESOURCES.len()).collect();
        }
        let keys: Vec<String> = candidates.iter().map(|i| format!("resource:{i}")).collect();
        candidates[self.pick(&keys)]
    }

    fn person(&mut self) -> &'static str {
        PEOPLE[self.below(PEOPLE.len())]
    }

    fn size_kb(&mut self) -> usize {
        4 + self.below(900)
    }

    fn next_build(&mut self) -> u64 {
        self.build += 1 + self.below(40) as u64;
        self.build
    }

    /// A link people paste while discussing the work: a ticket, the pull request, the
    /// project site or a reference page.
    fn ref_link(&mut self, c: &Ctx) -> String {
        let p = c.p;
        let options = ["ticket", "hash", "site0", "site1", "resource"];
        let keys: Vec<String> = options
            .iter()
            .map(|o| format!("ref-link:{}:{o}", p.slug))
            .collect();
        match options[self.pick(&keys)] {
            "ticket" => {
                let t = self.below(2);
                p.ticket_url(&p.tickets[t])
            }
            "hash" => p.ticket_url(&p.tickets[2]),
            "site0" => p.site_url(0),
            "site1" => p.site_url(1),
            _ => RESOURCES[self.pick_resource(Some(p.lang))].url.to_string(),
        }
    }

    fn ci_link(&mut self, c: &Ctx) -> String {
        let build = self.next_build();
        format!("https://{CI}/{}/builds/{build}", c.p.slug)
    }

    /// A link people paste while operating the service: CI, the error tracker, a dashboard.
    fn ops_link(&mut self, c: &Ctx) -> String {
        let p = c.p;
        let options = ["ci", "sentry", "grafana"];
        let keys: Vec<String> = options
            .iter()
            .map(|o| format!("ops-link:{}:{o}", p.slug))
            .collect();
        match options[self.pick(&keys)] {
            "ci" => self.ci_link(c),
            "sentry" => format!(
                "https://{SENTRY}/issues/{}/?project={}",
                4_400_000 + self.below(90_000),
                p.slug
            ),
            _ => format!(
                "https://{GRAFANA}/d/{}-overview/{}-overview?orgId=1",
                p.slug, p.slug
            ),
        }
    }

    /// Draws one frame. `sticky` carries what the session kept on screen.
    pub(crate) fn render(
        &mut self,
        session: &SessionPlan,
        frame: &FramePlan,
        echo: Option<&str>,
        sticky: &mut Sticky,
    ) -> Screen {
        let pi = session
            .project
            .unwrap_or_else(|| self.below(PROJECTS.len()));
        let c = Ctx {
            p: &PROJECTS[pi],
            pi,
            has_project: session.project.is_some(),
            activity: session.activity,
            at: frame.at,
            echo,
        };
        if let Some(anchor) = frame.anchor {
            return self.anchor_screen(&c, anchor, sticky);
        }
        match frame.app {
            App::Code => self.code(&c, sticky),
            App::Terminal => {
                if c.activity == Activity::AiPairing {
                    self.claude_terminal(&c, sticky)
                } else {
                    self.terminal(&c, sticky)
                }
            }
            App::Chrome => {
                let page = self.choose_page(&c, sticky);
                self.chrome(&c, page, sticky)
            }
            App::Slack => self.slack(&c),
            App::Outlook => self.outlook(&c),
            App::Excel => {
                let doc = self.choose_doc(&c, sticky, &["xlsx", "csv"], &[0, 4]);
                self.excel(&c, doc)
            }
            App::Word => {
                let doc = self.choose_doc(&c, sticky, &["docx"], &[2]);
                self.word(&c, doc)
            }
            App::PowerPoint => {
                let doc = self.choose_doc(&c, sticky, &["pptx"], &[5]);
                self.powerpoint(&c, doc)
            }
            App::Acrobat => {
                let doc = self.choose_doc(&c, sticky, &["pdf"], &[1, 3]);
                self.acrobat(&c, doc)
            }
            App::Notion => self.notion(&c),
            App::Figma => self.figma(&c),
            App::Explorer => {
                if c.has_project && self.chance(2, 3) {
                    self.explorer_project(&c)
                } else {
                    let folder = self.below(admin_folders().len());
                    self.explorer_admin(&c, folder)
                }
            }
            App::Zoom => self.zoom(&c),
        }
    }

    fn anchor_screen(&mut self, c: &Ctx, anchor: Anchor, sticky: &mut Sticky) -> Screen {
        match anchor {
            Anchor::Code(_) => {
                let mut body = git_remote(c.p);
                body.extend(rg_listing(c.p));
                let f = self.pick_file(c);
                sticky.file = Some(f);
                body.extend(git_status(c.p, f, &mut self.rng));
                self.terminal_screen(c, body)
            }
            Anchor::Review(_) => {
                sticky.page = Some(Page::CodeHost);
                self.chrome(c, Page::CodeHost, sticky)
            }
            Anchor::Triage(_) => {
                sticky.page = Some(Page::Board);
                self.chrome(c, Page::Board, sticky)
            }
            Anchor::Files(_) => self.explorer_project(c),
            Anchor::Site(_) => {
                let i = self.below(2);
                sticky.page = Some(Page::Site(i));
                self.chrome(c, Page::Site(i), sticky)
            }
            Anchor::Resource(i) => {
                sticky.page = Some(Page::Resource(i));
                self.chrome(c, Page::Resource(i), sticky)
            }
            Anchor::Observability(which, _) => {
                let page = [Page::Sentry, Page::Grafana, Page::Ci, Page::PagerDuty][which % 4];
                sticky.page = Some(page);
                self.chrome(c, page, sticky)
            }
            Anchor::Claude(_) => {
                sticky.page = Some(Page::Claude);
                self.chrome(c, Page::Claude, sticky)
            }
            Anchor::AdminFolder(folder) => self.explorer_admin(c, folder),
        }
    }

    // ----------------------------------------------------------------------------------
    // Editor and terminal
    // ----------------------------------------------------------------------------------

    fn sticky_file(&mut self, c: &Ctx, sticky: &mut Sticky) -> usize {
        match sticky.file {
            Some(f) if self.chance(3, 5) => f,
            _ => {
                let f = self.pick_file(c);
                sticky.file = Some(f);
                sticky.scroll = self.below(3);
                f
            }
        }
    }

    fn code(&mut self, c: &Ctx, sticky: &mut Sticky) -> Screen {
        let p = c.p;
        let f = self.sticky_file(c, sticky);
        if self.chance(1, 3) {
            sticky.scroll = (sticky.scroll + 1) % 3;
        }
        let rows = 12 + self.below(5);
        let (view, first) = viewport(p, f, sticky.scroll, rows);
        let other = (f + 1 + self.below(3)) % 4;

        let mut body = vec![p.files[f].replace('\\', " > ")];
        body.extend(view);
        body.push("PROBLEMS OUTPUT DEBUG CONSOLE TERMINAL PORTS".to_string());
        match self.below(4) {
            0 => {
                let failing = self.chance(1, 2);
                body.extend(test_run(p, f, failing, &mut self.rng));
            }
            1 => body.extend(git_status(p, f, &mut self.rng)),
            2 => body.extend(rg_listing(p).into_iter().take(3)),
            _ => body.push(prompt(
                p,
                &format!("code -g {}:{}", p.file_path(f), first + 2),
            )),
        }
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }

        let mut side = vec!["EXPLORER".to_string(), p.slug.to_ascii_uppercase()];
        for i in 0..4 {
            side.push(p.file_name(i).to_string());
        }
        side.extend(["OUTLINE".to_string(), "TIMELINE".to_string()]);

        Screen {
            app_hint: App::Code.hint(),
            title: format!("{} - {} - Visual Studio Code", p.file_path(f), p.slug),
            title_bar: true,
            top: vec![
                "File Edit Selection View Go Run Terminal Help".to_string(),
                format!("{} {}", p.file_name(f), p.file_name(other)),
            ],
            side,
            body,
            bottom: vec![
                format!("fix/{}", p.tickets[0].key.to_ascii_lowercase()),
                format!(
                    "Ln {}, Col {}",
                    first + self.below(rows),
                    1 + self.below(60)
                ),
                format!(
                    "Spaces: 4 UTF-8 CRLF {}",
                    lang_label(p.lang, p.file_name(f))
                ),
            ],
            ..Screen::default()
        }
    }

    fn terminal_screen(&mut self, c: &Ctx, body: Vec<String>) -> Screen {
        Screen {
            app_hint: App::Terminal.hint(),
            title: format!("{} - PowerShell", c.p.dir),
            title_bar: true,
            body,
            ..Screen::default()
        }
    }

    fn terminal(&mut self, c: &Ctx, sticky: &mut Sticky) -> Screen {
        let p = c.p;
        let f = self.sticky_file(c, sticky);
        let mut body: Vec<String> = Vec::new();
        let mut block_ends: Vec<usize> = Vec::new();
        let blocks = 2 + self.below(2);
        let mut kinds: Vec<usize> = (0..6).collect();
        self.rng.shuffle(&mut kinds);
        for kind in kinds.into_iter().take(blocks) {
            match kind {
                0 => {
                    let failing = self.chance(1, 2);
                    body.extend(test_run(p, f, failing, &mut self.rng));
                }
                1 => body.extend(git_status(p, f, &mut self.rng)),
                2 => body.extend(git_log(p, &mut self.rng)),
                3 => body.extend(git_remote(p)),
                4 => body.extend(rg_listing(p)),
                _ => {
                    body.push(prompt(
                        p,
                        &format!("Get-Content {} -Tail 3", p.file_path(f)),
                    ));
                    let lines = file_body(p, f);
                    body.extend(
                        lines
                            .iter()
                            .skip(lines.len().saturating_sub(3))
                            .map(|l| l.trim().to_string()),
                    );
                }
            }
            block_ends.push(body.len());
        }
        // A log line printed by the last command of a block, before the next prompt.
        if let Some(echo) = c.echo {
            let at = block_ends[self.below(block_ends.len())];
            body.insert(at, echo.to_string());
        }
        self.terminal_screen(c, body)
    }

    fn claude_terminal(&mut self, c: &Ctx, sticky: &mut Sticky) -> Screen {
        let p = c.p;
        let f = self.sticky_file(c, sticky);
        let t = match sticky.ticket {
            Some(t) => t,
            None => {
                let t = self.pick_ticket(c);
                sticky.ticket = Some(t);
                t
            }
        };
        let mut body = vec!["Claude Code".to_string(), format!("cwd: {}", p.dir)];
        if let Some(echo) = c.echo {
            body.push("> this is the log line we keep seeing:".to_string());
            body.push(echo.to_string());
        }
        body.extend(claude_session(p, f, &p.tickets[t], &mut self.rng));
        Screen {
            app_hint: App::Terminal.hint(),
            title: format!("Claude Code - {} {}", p.tickets[t].key, p.slug),
            title_bar: true,
            body,
            bottom: vec!["? for shortcuts".to_string()],
            ..Screen::default()
        }
    }

    // ----------------------------------------------------------------------------------
    // Browser
    // ----------------------------------------------------------------------------------

    fn choose_page(&mut self, c: &Ctx, sticky: &mut Sticky) -> Page {
        if let Some(page) = sticky.page {
            if self.chance(1, 2) {
                return page;
            }
        }
        let lang = c.p.lang;
        let page = if !c.has_project {
            Page::Resource(self.pick_resource(None))
        } else {
            let roll = self.below(100);
            match c.activity {
                Activity::Review => match roll {
                    0..=59 => Page::CodeHost,
                    60..=79 => Page::Ci,
                    _ => Page::Ticket(self.below(2)),
                },
                Activity::Triage => match roll {
                    0..=59 => Page::Ticket(self.below(2)),
                    60..=89 => Page::Board,
                    _ => Page::CodeHost,
                },
                Activity::Incident => match roll {
                    0..=34 => Page::Sentry,
                    35..=69 => Page::Grafana,
                    70..=84 => Page::Ci,
                    _ => Page::PagerDuty,
                },
                Activity::Research => match roll {
                    0..=69 => Page::Resource(self.pick_resource(Some(lang))),
                    _ => Page::Site(self.below(2)),
                },
                Activity::BrowserAi => match roll {
                    0..=84 => Page::Claude,
                    _ => Page::Resource(self.pick_resource(Some(lang))),
                },
                _ => match roll {
                    0..=39 => Page::Resource(self.pick_resource(Some(lang))),
                    40..=69 => Page::Ticket(self.below(2)),
                    70..=84 => Page::Site(self.below(2)),
                    _ => Page::CodeHost,
                },
            }
        };
        sticky.page = Some(page);
        page
    }

    fn chrome(&mut self, c: &Ctx, page: Page, sticky: &mut Sticky) -> Screen {
        let (title, url, mut body) = self.page(c, page, sticky);
        if let Some(echo) = c.echo {
            let at = 1.min(body.len());
            body.insert(at, echo.to_string());
        }
        let tab: String = title.chars().take(30).collect();
        Screen {
            app_hint: App::Chrome.hint(),
            title: format!("{title} - Google Chrome"),
            url: Some(url.clone()),
            top: vec![
                tab.trim_end().to_string(),
                url,
                "Jira GitHub Grafana Sentry Docs Calendar".to_string(),
            ],
            body,
            ..Screen::default()
        }
    }

    fn page(&mut self, c: &Ctx, page: Page, sticky: &mut Sticky) -> (String, String, Vec<String>) {
        let p = c.p;
        match page {
            Page::Ticket(t) => self.ticket_page(c, &p.tickets[t.min(1)]),
            Page::Board => {
                let prefix = p.tickets[0].key.split('-').next().unwrap_or("");
                let (title, url) = match p.tracker {
                    Tracker::Jira(site) => (
                        format!("{prefix} board - Agile Board - Jira"),
                        format!(
                            "https://{site}/jira/software/projects/{prefix}/boards/{}",
                            10 + c.pi
                        ),
                    ),
                    Tracker::Linear(ws) => (
                        format!("Active issues - {}", p.name),
                        format!("https://linear.app/{ws}/team/{prefix}/active"),
                    ),
                };
                let mut body = vec![
                    format!("{} board", p.name),
                    "TO DO IN PROGRESS IN REVIEW DONE".to_string(),
                ];
                for ticket in &p.tickets[..2] {
                    body.push(format!(
                        "{} {} {}",
                        ticket.key,
                        ticket.title,
                        teammate(p, &mut self.rng)
                    ));
                }
                body.push(format!(
                    "Linked pull request {} {}",
                    p.tickets[2].key, p.tickets[2].title
                ));
                body.push(format!("Sprint {} ends Friday", 30 + self.below(20)));
                (title, url, body)
            }
            Page::CodeHost => self.code_host_page(c),
            Page::Ci => {
                let build = self.next_build();
                let f = self.pick_file(c);
                let mut body = ci_steps(p, build);
                body.push(format!(
                    "FAILED {} {}",
                    p.file_url_path(f),
                    camel(stem_of(p.file_name(f)))
                ));
                body.push(format!(
                    "Triggered by {} for {}",
                    teammate(p, &mut self.rng),
                    p.tickets[2].key
                ));
                (
                    format!("Build {build} - {} - Northwind CI", p.slug),
                    format!("https://{CI}/{}/builds/{build}", p.slug),
                    body,
                )
            }
            Page::Sentry => {
                let issue = 4_400_000 + self.below(90_000);
                let f = self.pick_file(c);
                let error = format!("{}{}Error", camel(p.vocab[0]), camel(p.vocab[2]));
                (
                    format!(
                        "{error}: {} {} exceeded - {} - Sentry",
                        p.vocab[1], p.vocab[0], p.slug
                    ),
                    format!("https://{SENTRY}/issues/{issue}/?project={}", p.slug),
                    vec![
                        error,
                        format!(
                            "{} {} exceeded for tenant {}",
                            p.vocab[1], p.vocab[0], p.service
                        ),
                        format!(
                            "Events {} Users {} First seen {} days ago",
                            100 + self.below(5000),
                            10 + self.below(900),
                            1 + self.below(9)
                        ),
                        format!(
                            "{} in {} at line {}",
                            p.file_url_path(f),
                            p.vocab[2],
                            10 + self.below(90)
                        ),
                        format!(
                            "Suspect commit {} for {}",
                            short_hash(&mut self.rng),
                            p.tickets[0].key
                        ),
                    ],
                )
            }
            Page::Grafana => (
                format!("{} overview - Dashboards - Grafana", p.name),
                format!(
                    "https://{GRAFANA}/d/{}-overview/{}-overview?orgId=1&from=now-6h",
                    p.slug, p.slug
                ),
                vec![
                    format!("{} overview", p.name),
                    format!(
                        "Request rate {}.{}k req/s",
                        1 + self.below(9),
                        self.below(10)
                    ),
                    format!("Error ratio {}.{}%", self.below(5), self.below(10)),
                    format!("p99 latency {} ms", 120 + self.below(900)),
                    format!("{} {} saturation by tenant", p.service, p.vocab[0]),
                ],
            ),
            Page::PagerDuty => {
                let incident = 1000 + self.below(9000);
                (
                    format!(
                        "Incident {incident} {} error budget burn - PagerDuty",
                        p.service
                    ),
                    format!("https://{PAGERDUTY}/incidents/Q{incident}ZP"),
                    vec![
                        format!("{} error budget burn rate above 14x", p.service),
                        format!("Triggered {} Acknowledged by {USER_NAME}", hm(c.at)),
                        format!("Escalation policy {} primary", p.name),
                        format!("Related: {}", p.tickets[0].key),
                    ],
                )
            }
            Page::Site(i) => {
                let doc = p.docs[self.pick_doc_index(c)];
                (
                    format!("{} - {}", humanize(p.site_paths[i]), p.name),
                    p.site_url(i),
                    vec![
                        format!("{} documentation", p.name),
                        humanize(p.site_paths[i]),
                        format!(
                            "This page explains how {} handles {} and {}.",
                            p.service, p.vocab[0], p.vocab[2]
                        ),
                        format!("Design notes: {doc}"),
                        format!("Open questions are tracked in {}", p.tickets[1].key),
                    ],
                )
            }
            Page::Resource(i) => {
                let r = &RESOURCES[i];
                let mut body: Vec<String> = r.lines.iter().map(|l| l.to_string()).collect();
                body.push(format!(
                    "Read while working on {}",
                    p.tickets[self.below(2)].key
                ));
                (r.title.to_string(), r.url.to_string(), body)
            }
            Page::Claude => {
                let chat = match &sticky.claude_chat {
                    Some(chat) => chat.clone(),
                    None => {
                        let a = self.rng.next_u64();
                        let b = self.rng.next_u64();
                        let chat = format!(
                            "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
                            a >> 32,
                            (a >> 16) & 0xffff,
                            a & 0xfff,
                            b >> 52,
                            b & 0xffff_ffff_ffff
                        );
                        sticky.claude_chat = Some(chat.clone());
                        chat
                    }
                };
                let ticket = &p.tickets[self.below(2)];
                let f = self.pick_file(c);
                (
                    format!("{} {} question - Claude", camel(p.vocab[0]), p.vocab[3]),
                    format!("https://{CLAUDE}/chat/{chat}"),
                    vec![
                        format!(
                            "How should {} combine {} with {}? Context: {}",
                            p.name, p.vocab[2], p.vocab[3], ticket.key
                        ),
                        format!(
                            "A common approach is to cap the {} first and then apply full {}.",
                            p.vocab[2], p.vocab[3]
                        ),
                        format!(
                            "In {} that means changing next_{}.",
                            p.file_url_path(f),
                            p.vocab[2]
                        ),
                        "Reply to Claude...".to_string(),
                    ],
                )
            }
        }
    }

    fn ticket_page(&mut self, c: &Ctx, ticket: &Ticket) -> (String, String, Vec<String>) {
        let p = c.p;
        let assignee = teammate(p, &mut self.rng);
        let reporter = self.person();
        let f = self.pick_file(c);
        let doc = p.docs[self.pick_doc_index(c)];
        let (title, crumbs) = match p.tracker {
            Tracker::Jira(_) => (
                format!("[{}] {} - Jira", ticket.key, ticket.title),
                format!("Projects / {} / {}", p.name, ticket.key),
            ),
            Tracker::Linear(_) => (
                format!("{} {}", ticket.key, ticket.title),
                format!("{} / {}", p.name, ticket.key),
            ),
        };
        let body = vec![
            crumbs,
            format!("{} {}", ticket.key, ticket.title),
            format!("Status In Progress Assignee {assignee} Reporter {reporter}"),
            format!(
                "Seen in {} on {}; see {doc} for the expected behaviour.",
                p.file_url_path(f),
                p.service
            ),
            format!("Development: {} {}", p.tickets[2].key, p.tickets[2].title),
            format!(
                "{assignee} {} I can reproduce it on main.",
                hm(c.at - 3_600_000)
            ),
        ];
        (title, p.ticket_url(ticket), body)
    }

    fn code_host_page(&mut self, c: &Ctx) -> (String, String, Vec<String>) {
        let p = c.p;
        let pr = &p.tickets[2];
        let n = pr.number();
        let author = teammate(p, &mut self.rng);
        let doc = p.docs[self.pick_doc_index(c)];
        let title = match p.host() {
            "github.com" => format!(
                "{} by {} - Pull Request #{n} - {}",
                pr.title,
                author
                    .split(' ')
                    .next()
                    .unwrap_or(author)
                    .to_ascii_lowercase(),
                p.owner_repo()
            ),
            "gitlab.com" => format!("{} (#{n}) - Issues - {} - GitLab", pr.title, p.owner_repo()),
            "bitbucket.org" => format!(
                "{} - Pull request #{n}: {} - Bitbucket",
                p.owner_repo(),
                pr.title
            ),
            _ => format!("{} - #{n} - {} - Codeberg.org", pr.title, p.owner_repo()),
        };
        let mut body = vec![
            format!("{} {}", pr.title, pr.key),
            format!(
                "Open {author} wants to merge 3 commits into main from fix/{}",
                p.tickets[0].key.to_ascii_lowercase()
            ),
            format!("Closes {}. Design notes in {doc}.", p.tickets[0].key),
            format!("Files changed {}", p.files.len()),
        ];
        for i in 0..p.files.len() {
            body.push(format!(
                "{} +{} -{}",
                p.file_url_path(i),
                1 + self.below(40),
                self.below(15)
            ));
        }
        body.push(format!(
            "{} commented: this also covers {}",
            teammate(p, &mut self.rng),
            p.tickets[1].key
        ));
        (title, p.ticket_url(pr), body)
    }

    // ----------------------------------------------------------------------------------
    // Messaging
    // ----------------------------------------------------------------------------------

    /// One chat message (one or two lines) about the project.
    fn message(&mut self, c: &Ctx) -> Vec<String> {
        let p = c.p;
        let t = self.pick_ticket(c);
        let ticket = p.tickets[t];
        match self.below(10) {
            0 => vec![format!(
                "{} is back in this sprint: {}",
                ticket.key,
                ticket.title.to_lowercase()
            )],
            1 => vec![format!(
                "PR is up for {}: {}",
                p.tickets[0].key,
                p.ticket_url(&p.tickets[2])
            )],
            2 => vec![format!("CI is red again on main: {}", self.ci_link(c))],
            3 => {
                let doc = p.docs[self.pick_doc_index(c)];
                let kb = self.size_kb();
                vec![
                    format!("Notes from today are in {doc}"),
                    format!("{doc} {kb} KB"),
                ]
            }
            4 => {
                let f = self.pick_file(c);
                vec![format!(
                    "Repro is in {} if anyone wants to look",
                    p.file_path(f)
                )]
            }
            5 => vec![format!(
                "Worth reading before we decide: {}",
                self.ref_link(c)
            )],
            6 => vec![format!(
                "{} and {} should land together, can someone review?",
                p.tickets[t.min(1)].key,
                p.tickets[2].key
            )],
            7 => vec![format!(
                "Deploying {} to staging now, watching {}",
                p.service,
                self.ops_link(c)
            )],
            8 => vec![format!("Clone is {} if you need it", p.https_clone_url())],
            _ => vec![format!("Updated the docs: {}", p.site_url(self.below(2)))],
        }
    }

    fn slack(&mut self, c: &Ctx) -> Screen {
        let p = c.p;
        let mut body = vec![
            format!("# {}", p.channel),
            format!("{} members", 6 + self.below(40)),
        ];
        let messages = 3 + self.below(3);
        let echo_at = self.below(messages);
        for m in 0..messages {
            let author = if self.chance(1, 4) {
                USER_NAME
            } else {
                teammate(p, &mut self.rng)
            };
            let minutes = (messages - m) as i64 * (2 + self.below(9) as i64);
            body.push(format!("{author} {}", hm(c.at - minutes * 60_000)));
            if m == echo_at {
                if let Some(echo) = c.echo {
                    body.push("Anyone seen this before?".to_string());
                    body.push(echo.to_string());
                    continue;
                }
            }
            body.extend(self.message(c));
        }
        let windowed = self.chance(3, 10);
        let mut side = vec![
            "Home".to_string(),
            "DMs".to_string(),
            "Activity".to_string(),
            "Channels".to_string(),
            format!("# {}", p.channel),
            "# general".to_string(),
            "# incidents".to_string(),
            "Direct messages".to_string(),
        ];
        side.extend(p.team.iter().map(|i| PEOPLE[*i].to_string()));
        Screen {
            app_hint: App::Slack.hint(),
            title: format!("{} (Channel) - Northwind Labs - Slack", p.channel),
            windowed,
            top: vec!["Search Northwind Labs".to_string()],
            side,
            body,
            bottom: vec![format!("Message # {}", p.channel)],
            background: if windowed {
                CALENDAR_BACKGROUND.iter().map(|s| s.to_string()).collect()
            } else {
                Vec::new()
            },
            ..Screen::default()
        }
    }

    fn outlook(&mut self, c: &Ctx) -> Screen {
        let p = c.p;
        let sender = if c.has_project {
            teammate(p, &mut self.rng)
        } else {
            self.person()
        };
        let (subject, mut lines) = if c.has_project {
            let t = self.pick_ticket(c);
            let ticket = p.tickets[t];
            match self.below(4) {
                // Tracker keys notify from the tracker; `#n` from the code host (below).
                0 if !ticket.is_hash() => (
                    format!(
                        "[{}] ({}) {}",
                        match p.tracker {
                            Tracker::Jira(_) => "JIRA",
                            Tracker::Linear(_) => "Linear",
                        },
                        ticket.key,
                        ticket.title
                    ),
                    vec![
                        format!("{sender} changed the status to In Review."),
                        p.ticket_url(&ticket),
                    ],
                ),
                0 | 1 => (
                    format!(
                        "[{}] {} {}",
                        p.owner_repo(),
                        p.tickets[2].title,
                        p.tickets[2].key
                    ),
                    vec![
                        format!("{sender} requested your review on {}.", p.tickets[2].key),
                        p.ticket_url(&p.tickets[2]),
                    ],
                ),
                2 => {
                    let doc = p.docs[self.pick_doc_index(c)];
                    let kb = self.size_kb();
                    (
                        format!("RE: {doc} review"),
                        vec![
                            format!(
                                "Hi {}, comments inline, mostly about {}.",
                                USER_NAME.split(' ').next().unwrap_or(USER_NAME),
                                p.vocab[2]
                            ),
                            format!("Attachment: {doc} ({kb} KB)"),
                        ],
                    )
                }
                _ => (
                    format!("Alert: {} error rate above 2% for 10 minutes", p.service),
                    vec![format!("Runbook: {}", p.site_url(0)), self.ops_link(c)],
                ),
            }
        } else {
            let doc = &ADMIN_DOCS[self.below(ADMIN_DOCS.len())];
            let kb = self.size_kb();
            (
                format!("{} - please review", humanize(doc.name)),
                vec![
                    doc.lines[0].to_string(),
                    format!("Attachment: {} ({kb} KB)", doc.name),
                ],
            )
        };
        if let Some(echo) = c.echo {
            lines.insert(0, echo.to_string());
        }
        let mut body = Vec::new();
        for i in 0..3 {
            let other = self.person();
            body.push(format!(
                "{other} {} {}",
                ["Weekly digest", "Lunch on Thursday?", "Build summary"][i],
                hm(c.at - (i as i64 + 1) * 1_800_000)
            ));
        }
        body.push(subject);
        body.push(format!("{sender} <{}>", email_of(sender)));
        body.push(format!("To: {USER_NAME} <{USER_EMAIL}>"));
        body.extend(lines);
        Screen {
            app_hint: App::Outlook.hint(),
            title: format!("Inbox - {USER_EMAIL} - Outlook"),
            title_bar: true,
            top: vec![
                "File Home View Help".to_string(),
                "New mail Delete Archive Reply Reply all Forward".to_string(),
            ],
            side: [
                "Favorites",
                "Inbox",
                "Sent Items",
                "Drafts",
                "Archive",
                "Notifications",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            body,
            bottom: vec![
                format!("Items: {}", 200 + self.below(100)),
                format!("Unread: {}", self.below(20)),
                "All folders are up to date.".to_string(),
            ],
            ..Screen::default()
        }
    }

    // ----------------------------------------------------------------------------------
    // Documents
    // ----------------------------------------------------------------------------------

    /// The session's open document: one of the project's documents with an accepted
    /// extension, otherwise one of the listed admin documents.
    fn choose_doc(&mut self, c: &Ctx, sticky: &mut Sticky, exts: &[&str], admin: &[usize]) -> Doc {
        if let Some(doc) = sticky.doc {
            let ext = doc.name().rsplit_once('.').map_or("", |(_, e)| e);
            if exts.contains(&ext) && self.chance(2, 3) {
                return doc;
            }
        }
        let own: Vec<usize> = if c.has_project {
            (0..3)
                .filter(|d| {
                    let ext = c.p.docs[*d].rsplit_once('.').map_or("", |(_, e)| e);
                    exts.contains(&ext)
                })
                .collect()
        } else {
            Vec::new()
        };
        let doc = if own.is_empty() {
            Doc::Admin(admin[self.below(admin.len())])
        } else {
            Doc::Project(c.pi, own[self.below(own.len())])
        };
        sticky.doc = Some(doc);
        doc
    }

    /// Body lines of a document: admin documents carry their own text, project documents
    /// are written about the project.
    fn doc_lines(&mut self, doc: Doc) -> Vec<String> {
        match doc {
            Doc::Admin(i) => ADMIN_DOCS[i].lines.iter().map(|l| l.to_string()).collect(),
            Doc::Project(p, d) => {
                let p = &PROJECTS[p];
                vec![
                    humanize(p.docs[d]),
                    format!(
                        "Owner: {} Reviewers: {}",
                        teammate(p, &mut self.rng),
                        USER_NAME
                    ),
                    format!(
                        "{} must keep {} and {} consistent across releases.",
                        p.name, p.vocab[0], p.vocab[1]
                    ),
                    format!("Open issues: {} and {}", p.tickets[0].key, p.tickets[1].key),
                    format!("Implementation lands with {}", p.tickets[2].key),
                    format!("Reference: {}", p.site_url(d % 2)),
                ]
            }
        }
    }

    fn excel(&mut self, c: &Ctx, doc: Doc) -> Screen {
        let mut body = vec![
            format!("D{} fx =SUM(D2:D{})", 8 + self.below(6), 7 + self.below(6)),
            "A B C D E F".to_string(),
        ];
        match doc {
            Doc::Admin(i) => body.extend(ADMIN_DOCS[i].lines.iter().map(|l| l.to_string())),
            Doc::Project(p, _) => {
                let p = &PROJECTS[p];
                body.push(format!(
                    "{} {} {} Owner Ticket",
                    camel(p.vocab[0]),
                    camel(p.vocab[1]),
                    camel(p.vocab[2])
                ));
                for row in 0..5 + self.below(4) {
                    body.push(format!(
                        "{}-{} {} {}.{} {} {}",
                        p.vocab[0],
                        row + 1,
                        100 + self.below(9000),
                        self.below(100),
                        self.below(100),
                        teammate(p, &mut self.rng),
                        p.tickets[row % 2].key
                    ));
                }
            }
        }
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }
        Screen {
            app_hint: App::Excel.hint(),
            title: format!("{} - Excel", doc.name()),
            title_bar: true,
            top: vec![
                "File Home Insert Page Layout Formulas Data Review View Help".to_string(),
                "AutoSave Off".to_string(),
            ],
            body,
            bottom: vec![
                "Summary Data Notes".to_string(),
                "Ready".to_string(),
                format!(
                    "Average: {}.{} Count: {} Sum: {}",
                    self.below(900),
                    self.below(10),
                    6 + self.below(30),
                    1000 + self.below(90_000)
                ),
            ],
            ..Screen::default()
        }
    }

    fn word(&mut self, c: &Ctx, doc: Doc) -> Screen {
        let mut body = self.doc_lines(doc);
        if let Some(echo) = c.echo {
            body.insert(1, echo.to_string());
        }
        let pages = 2 + self.below(10);
        Screen {
            app_hint: App::Word.hint(),
            title: format!("{} - Word", doc.name()),
            title_bar: true,
            top: vec![
                "File Home Insert Layout References Review View Help".to_string(),
                "AutoSave Off".to_string(),
            ],
            body,
            bottom: vec![format!(
                "Page {} of {pages} {} words English (United States)",
                1 + self.below(pages),
                300 + self.below(4000)
            )],
            ..Screen::default()
        }
    }

    fn powerpoint(&mut self, c: &Ctx, doc: Doc) -> Screen {
        let mut body = self.doc_lines(doc);
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }
        let slides = 8 + self.below(14);
        Screen {
            app_hint: App::PowerPoint.hint(),
            title: format!("{} - PowerPoint", doc.name()),
            title_bar: true,
            top: vec![
                "File Home Insert Design Transitions Animations Slide Show Review View".to_string(),
            ],
            side: (1..=8).map(|i| i.to_string()).collect(),
            body,
            bottom: vec![format!("Slide {} of {slides}", 1 + self.below(slides))],
            ..Screen::default()
        }
    }

    fn acrobat(&mut self, c: &Ctx, doc: Doc) -> Screen {
        let mut body = self.doc_lines(doc);
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }
        let pages = 3 + self.below(20);
        Screen {
            app_hint: App::Acrobat.hint(),
            title: format!("{} - Adobe Acrobat Reader (64-bit)", doc.name()),
            title_bar: true,
            top: vec![
                "Home Tools".to_string(),
                "All tools Edit Convert Sign".to_string(),
            ],
            body,
            bottom: vec![format!("{} / {pages}", 1 + self.below(pages))],
            ..Screen::default()
        }
    }

    fn notion(&mut self, c: &Ctx) -> Screen {
        let p = c.p;
        let topic = ["notes", "decision log", "weekly sync", "retro"][self.below(4)];
        let page_title = format!("{} {topic}", p.name);
        // The page heading repeats the window title; Notion draws it in the top bar, where
        // the filter drops it as a title echo, so it is the title bar here.
        let mut body = vec![format!(
            "Owner {} Status In progress Tickets {} {}",
            teammate(p, &mut self.rng),
            p.tickets[0].key,
            p.tickets[1].key
        )];
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }
        for _ in 0..2 + self.below(3) {
            body.extend(self.message(c));
        }
        let windowed = self.chance(1, 4);
        Screen {
            app_hint: App::Notion.hint(),
            title: page_title,
            title_bar: true,
            windowed,
            side: vec![
                "Search".to_string(),
                "Home".to_string(),
                "Inbox".to_string(),
                "Teamspaces".to_string(),
                p.name.to_string(),
                "Reliability".to_string(),
            ],
            body,
            background: if windowed {
                CALENDAR_BACKGROUND.iter().map(|s| s.to_string()).collect()
            } else {
                Vec::new()
            },
            ..Screen::default()
        }
    }

    fn figma(&mut self, c: &Ctx) -> Screen {
        let p = c.p;
        let screen_name = format!(
            "{} {}",
            p.name,
            ["settings", "checkout", "dashboard", "onboarding"][self.below(4)]
        );
        let mut body = vec![
            format!("{screen_name} / Desktop"),
            format!("{} / Focus / Dark", camel(p.vocab[1])),
            format!(
                "{} {} comment",
                teammate(p, &mut self.rng),
                hm(c.at - 1_200_000)
            ),
            format!(
                "{}: the {} state needs 3:1 contrast against the {} background",
                p.tickets[0].key, p.vocab[2], p.vocab[3]
            ),
        ];
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }
        Screen {
            app_hint: App::Figma.hint(),
            title: format!("{screen_name} - Figma"),
            side: vec![
                "Layers".to_string(),
                "Assets".to_string(),
                "Page 1".to_string(),
                "Header".to_string(),
                "Sidebar".to_string(),
                "Content".to_string(),
            ],
            top: vec!["Design Prototype Dev Mode Share".to_string()],
            body,
            ..Screen::default()
        }
    }

    fn explorer_screen(&mut self, folder: String, rows: Vec<String>, c: &Ctx) -> Screen {
        let count = rows.len();
        let mut body = vec![folder.clone(), "Name Date modified Type Size".to_string()];
        body.extend(rows);
        if let Some(echo) = c.echo {
            body.push(echo.to_string());
        }
        Screen {
            app_hint: App::Explorer.hint(),
            title: format!("{folder} - File Explorer"),
            title_bar: true,
            top: vec!["New Cut Copy Paste Rename Share Delete Sort View".to_string()],
            side: [
                "Home",
                "Gallery",
                "Desktop",
                "Downloads",
                "Documents",
                "Pictures",
                "This PC",
                "Network",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            body,
            bottom: vec![format!("{count} items"), "1 item selected".to_string()],
            ..Screen::default()
        }
    }

    fn file_row(&mut self, name: &str, at: i64) -> String {
        let kind = match name.rsplit_once('.').map_or("", |(_, e)| e) {
            "md" => "Markdown File",
            "pdf" => "Adobe Acrobat Document",
            "docx" => "Microsoft Word Document",
            "xlsx" => "Microsoft Excel Worksheet",
            "pptx" => "Microsoft PowerPoint Presentation",
            "csv" => "Microsoft Excel Comma Separated Values File",
            _ => "File",
        };
        let days = 1 + self.below(20) as i64;
        let when = local(at - days * 86_400_000).format("%d/%m/%Y %H:%M");
        format!("{name} {when} {kind} {} KB", self.size_kb())
    }

    fn explorer_project(&mut self, c: &Ctx) -> Screen {
        let p = c.p;
        let folder = format!(r"{}\docs", p.dir);
        let mut rows = Vec::new();
        for doc in p.docs {
            rows.push(self.file_row(doc, c.at));
        }
        rows.push(self.file_row("architecture.png", c.at));
        self.explorer_screen(folder, rows, c)
    }

    fn explorer_admin(&mut self, c: &Ctx, folder: usize) -> Screen {
        let folders = admin_folders();
        let folder = folders[folder % folders.len()];
        let docs: Vec<&AdminDoc> = ADMIN_DOCS.iter().filter(|d| d.folder == folder).collect();
        let mut rows = Vec::new();
        for doc in docs {
            rows.push(self.file_row(doc.name, c.at));
        }
        rows.push(self.file_row("receipt-scan.jpg", c.at));
        self.explorer_screen(folder.to_string(), rows, c)
    }

    fn zoom(&mut self, c: &Ctx) -> Screen {
        let p = c.p;
        let mut body = vec![format!("{} weekly sync", p.name)];
        for i in p.team {
            body.push(PEOPLE[i].to_string());
        }
        body.push(USER_NAME.to_string());
        body.push(format!(
            "{} is sharing the screen",
            teammate(p, &mut self.rng)
        ));
        let doc = p.docs[self.pick_doc_index(c)];
        body.push(humanize(doc));
        body.push(format!(
            "Open items: {} {}",
            p.tickets[0].key, p.tickets[1].key
        ));
        body.push(format!("From {} to Everyone:", teammate(p, &mut self.rng)));
        match c.echo {
            Some(echo) => body.push(echo.to_string()),
            None => {
                let link = self.ref_link(c);
                body.push(link);
            }
        }
        Screen {
            app_hint: App::Zoom.hint(),
            title: "Zoom Meeting".to_string(),
            top: vec!["View".to_string()],
            body,
            bottom: [
                "Mute",
                "Start Video",
                "Participants",
                "Chat",
                "Share",
                "Record",
                "Reactions",
                "Leave",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            ..Screen::default()
        }
    }
}

/// `retry-policy-design.md` -> `Retry policy design`, `/guides/retry-budgets` -> `Retry budgets`.
fn humanize(name: &str) -> String {
    let last = name
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(name);
    let stem = match last.rsplit_once('.') {
        Some((stem, ext)) if ["md", "pdf", "docx", "xlsx", "pptx", "csv"].contains(&ext) => stem,
        _ => last,
    };
    let words: Vec<&str> = stem.split(['-', '_']).filter(|w| !w.is_empty()).collect();
    let mut out = words.join(" ");
    if let Some(first) = out.get(..1) {
        out = first.to_ascii_uppercase() + &out[1..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_reads_like_a_heading() {
        assert_eq!(humanize("retry-policy-design.md"), "Retry policy design");
        assert_eq!(humanize("/guides/retry-budgets"), "Retry budgets");
        assert_eq!(humanize("/release/5.12"), "5.12");
    }
}
