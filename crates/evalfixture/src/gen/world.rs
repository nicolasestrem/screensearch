//! The fixture's world: a fixed, documented catalog of projects, people and web resources.
//!
//! The catalog is **seed-independent**: every fixture draws from the same entities, and the
//! seed only decides when, where and how often each one shows up. That keeps the entity set
//! inspectable (and testable through [`world_entities`]) while the content still changes with
//! the seed. Every string here is synthetic: the companies are the conventional fictional
//! Northwind/Contoso/Fabrikam names, people and the local user (`alex`) are invented, and the
//! public sites are only referenced by URL.
//!
//! Each project couples a repository (`<host>/<owner>/<repo>`, a code host the harness
//! extractor recognises), a local clone (an absolute Windows path), four source files, three
//! ticket references (two `ABC-123` keys and one `#123`), three documents and a project site,
//! so a frame about one project naturally mixes its files, tickets, documents and domains.

/// The synthetic local user. Paths live under `C:\Users\alex`; nothing names a real account.
pub(crate) const USER_NAME: &str = "Alex Morgan";
pub(crate) const USER_EMAIL: &str = "alex.morgan@northwind.example";

/// Colleagues who author messages, comments and meeting chat.
pub(crate) const PEOPLE: [&str; 10] = [
    "Ana Ruiz",
    "Ben Okafor",
    "Chloe Martin",
    "Dev Patel",
    "Emma Larsen",
    "Farid Haddad",
    "Grace Liu",
    "Hugo Bernard",
    "Ines Costa",
    "Jonas Weber",
];

/// Source language of a project, which decides its terminal output and test runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    Rust,
    Python,
    TypeScript,
    Go,
    CSharp,
    Kotlin,
    Terraform,
    Docs,
}

/// One ticket reference and the title it is filed under.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ticket {
    pub key: &'static str,
    pub title: &'static str,
}

impl Ticket {
    /// `#123` references live on the repository host; `ABC-123` keys live on the tracker.
    pub(crate) fn is_hash(&self) -> bool {
        self.key.starts_with('#')
    }

    /// The number of a `#123` reference (`123`), or the full key for tracker keys.
    pub(crate) fn number(&self) -> &'static str {
        self.key.trim_start_matches('#')
    }
}

/// Where a project's `ABC-123` tickets are tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tracker {
    /// Jira Cloud at `https://<site>/browse/<KEY>`.
    Jira(&'static str),
    /// Linear at `https://linear.app/<workspace>/issue/<KEY>`.
    Linear(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Project {
    pub slug: &'static str,
    pub name: &'static str,
    /// `<host>/<owner>/<repo>`.
    pub repo: &'static str,
    /// Local clone, an absolute Windows path.
    pub dir: &'static str,
    pub lang: Lang,
    /// Paths relative to [`Project::dir`], Windows separators.
    pub files: [&'static str; 4],
    pub tickets: [Ticket; 3],
    pub docs: [&'static str; 3],
    /// The project's own site (docs, admin UI or status), and two paths on it.
    pub site: &'static str,
    pub site_paths: [&'static str; 2],
    pub tracker: Tracker,
    pub channel: &'static str,
    /// Indices into [`PEOPLE`].
    pub team: [usize; 3],
    /// Domain nouns the code, logs and messages are written about.
    pub vocab: [&'static str; 4],
    /// The deployed service name used in alerts and dashboards.
    pub service: &'static str,
}

impl Project {
    pub(crate) fn host(&self) -> &'static str {
        self.repo.split('/').next().unwrap_or(self.repo)
    }

    /// `<owner>/<repo>`.
    pub(crate) fn owner_repo(&self) -> &'static str {
        self.repo
            .split_once('/')
            .map_or(self.repo, |(_, rest)| rest)
    }

    pub(crate) fn file_path(&self, index: usize) -> String {
        format!(r"{}\{}", self.dir, self.files[index])
    }

    /// A file path as code hosts and CI print it (forward slashes, repo-relative).
    pub(crate) fn file_url_path(&self, index: usize) -> String {
        self.files[index].replace('\\', "/")
    }

    /// The bare file name of `files[index]`.
    pub(crate) fn file_name(&self, index: usize) -> &'static str {
        let file = self.files[index];
        file.rsplit('\\').next().unwrap_or(file)
    }

    pub(crate) fn https_clone_url(&self) -> String {
        format!("https://{}.git", self.repo)
    }

    /// The web page for a ticket: tracker keys on the tracker, `#n` on the code host.
    pub(crate) fn ticket_url(&self, ticket: &Ticket) -> String {
        if ticket.is_hash() {
            let n = ticket.number();
            match self.host() {
                "github.com" => format!("https://{}/pull/{n}", self.repo),
                "gitlab.com" => format!("https://{}/-/issues/{n}", self.repo),
                "bitbucket.org" => format!("https://{}/pull-requests/{n}", self.repo),
                _ => format!("https://{}/issues/{n}", self.repo),
            }
        } else {
            match self.tracker {
                Tracker::Jira(site) => format!("https://{site}/browse/{}", ticket.key),
                Tracker::Linear(workspace) => {
                    format!("https://linear.app/{workspace}/issue/{}", ticket.key)
                }
            }
        }
    }

    pub(crate) fn site_url(&self, index: usize) -> String {
        format!("https://{}{}", self.site, self.site_paths[index])
    }

    pub(crate) fn tracker_domain(&self) -> &'static str {
        match self.tracker {
            Tracker::Jira(site) => site,
            Tracker::Linear(_) => "linear.app",
        }
    }

    /// The identifier form of the slug (`skyline-etl` -> `skyline_etl`).
    pub(crate) fn ident(&self) -> String {
        self.slug.replace('-', "_")
    }
}

const fn t(key: &'static str, title: &'static str) -> Ticket {
    Ticket { key, title }
}

pub(crate) const PROJECTS: [Project; 12] = [
    Project {
        slug: "ledgerline",
        name: "Ledgerline",
        repo: "github.com/northwind-labs/ledgerline",
        dir: r"C:\dev\ledgerline",
        lang: Lang::Rust,
        files: [
            r"src\retry.rs",
            r"src\budget.rs",
            r"src\http\client.rs",
            r"tests\retry_budget.rs",
        ],
        tickets: [
            t("LED-142", "Retry budget ignores jitter on 429 responses"),
            t("LED-157", "Expose retry budget metrics per tenant"),
            t("#418", "Cap backoff at max_delay and add full jitter"),
        ],
        docs: [
            "retry-policy-design.md",
            "ledgerline-runbook.md",
            "q3-reliability-review.pptx",
        ],
        site: "docs.ledgerline.dev",
        site_paths: ["/guides/retry-budgets", "/reference/client-config"],
        tracker: Tracker::Jira("northwind.atlassian.net"),
        channel: "ledgerline-dev",
        team: [0, 1, 3],
        vocab: ["budget", "retry", "backoff", "jitter"],
        service: "ledgerline-api",
    },
    Project {
        slug: "tallyhub",
        name: "TallyHub",
        repo: "github.com/northwind-labs/tallyhub",
        dir: r"C:\dev\tallyhub",
        lang: Lang::Python,
        files: [
            r"tallyhub\ingest.py",
            r"tallyhub\models.py",
            r"tallyhub\api\routes.py",
            r"tests\test_ingest.py",
        ],
        tickets: [
            t(
                "TAL-88",
                "CSV ingest drops rows that start with a BOM header",
            ),
            t("TAL-91", "Paginate the entries endpoint"),
            t("#233", "Switch ingest to a streaming CSV parser"),
        ],
        docs: [
            "ingest-format-spec.md",
            "tally-schema-v2.pdf",
            "sample-entries.csv",
        ],
        site: "tallyhub.northwind.dev",
        site_paths: ["/admin/imports", "/api/docs/entries"],
        tracker: Tracker::Jira("northwind.atlassian.net"),
        channel: "tallyhub",
        team: [2, 4, 7],
        vocab: ["entry", "ingest", "row", "ledger"],
        service: "tallyhub-ingest",
    },
    Project {
        slug: "fieldnotes",
        name: "Fieldnotes",
        repo: "gitlab.com/fabrikam/fieldnotes",
        dir: r"C:\dev\fieldnotes",
        lang: Lang::TypeScript,
        files: [
            r"src\sync\queue.ts",
            r"src\sync\conflict.ts",
            r"src\ui\NoteList.tsx",
            r"test\queue.spec.ts",
        ],
        tickets: [
            t(
                "FN-310",
                "Offline queue replays edits twice after reconnect",
            ),
            t("FN-322", "Conflict banner overlaps the toolbar"),
            t("#557", "Drop the legacy IndexedDB adapter"),
        ],
        docs: [
            "sync-protocol.md",
            "offline-ux-notes.docx",
            "release-checklist.md",
        ],
        site: "fieldnotes.fabrikam.app",
        site_paths: ["/app/notes", "/status"],
        tracker: Tracker::Linear("fabrikam"),
        channel: "fieldnotes-eng",
        team: [5, 8, 9],
        vocab: ["queue", "note", "sync", "conflict"],
        service: "fieldnotes-sync",
    },
    Project {
        slug: "orbitctl",
        name: "Orbit",
        repo: "github.com/contoso/orbitctl",
        dir: r"C:\dev\orbitctl",
        lang: Lang::Go,
        files: [
            r"cmd\orbitctl\main.go",
            r"internal\deploy\rollout.go",
            r"internal\deploy\health.go",
            r"internal\deploy\rollout_test.go",
        ],
        tickets: [
            t("ORB-512", "Rollout stalls when the health probe times out"),
            t("ORB-530", "Add a dry-run flag to promote"),
            t("#1290", "Retry health probes with exponential backoff"),
        ],
        docs: [
            "rollout-strategy.md",
            "incident-2026-09-14.md",
            "oncall-rotation.xlsx",
        ],
        site: "orbit.contoso.io",
        site_paths: ["/rollouts/payments-api", "/clusters/eu-west-1"],
        tracker: Tracker::Jira("contoso.atlassian.net"),
        channel: "orbit-ops",
        team: [1, 6, 9],
        vocab: ["rollout", "probe", "cluster", "phase"],
        service: "orbit-controller",
    },
    Project {
        slug: "paylane",
        name: "Paylane",
        repo: "bitbucket.org/contoso-finance/paylane",
        dir: r"C:\dev\paylane",
        lang: Lang::CSharp,
        files: [
            r"src\Paylane.Api\Controllers\InvoiceController.cs",
            r"src\Paylane.Core\Settlement.cs",
            r"src\Paylane.Core\Currency.cs",
            r"tests\Paylane.Tests\SettlementTests.cs",
        ],
        tickets: [
            t(
                "PAY-77",
                "Settlement rounding differs from the ledger by one cent",
            ),
            t("PAY-81", "Invoice PDF is missing the VAT id"),
            t("#664", "Use banker's rounding in settlement totals"),
        ],
        docs: [
            "settlement-rules.pdf",
            "vat-invoice-template.docx",
            "reconciliation-sept.xlsx",
        ],
        site: "paylane.contoso.io",
        site_paths: ["/invoices/20931", "/settlements/2026-09"],
        tracker: Tracker::Jira("contoso.atlassian.net"),
        channel: "paylane",
        team: [3, 4, 8],
        vocab: ["invoice", "settlement", "currency", "amount"],
        service: "paylane-api",
    },
    Project {
        slug: "lumen-ui",
        name: "Lumen UI",
        repo: "github.com/northwind-labs/lumen-ui",
        dir: r"C:\dev\lumen-ui",
        lang: Lang::TypeScript,
        files: [
            r"src\components\Button.tsx",
            r"src\tokens\color.ts",
            r"src\components\DatePicker.tsx",
            r"stories\Button.stories.tsx",
        ],
        tickets: [
            t("LUM-204", "Focus ring is invisible on the dark theme"),
            t("LUM-219", "DatePicker traps keyboard focus"),
            t("#772", "Migrate design tokens to CSS variables"),
        ],
        docs: [
            "design-tokens-v3.md",
            "accessibility-audit-2026.pdf",
            "component-inventory.xlsx",
        ],
        site: "lumen.northwind.design",
        site_paths: ["/components/button", "/tokens/color"],
        tracker: Tracker::Jira("northwind.atlassian.net"),
        channel: "design-system",
        team: [0, 6, 2],
        vocab: ["token", "button", "focus", "theme"],
        service: "lumen-docs",
    },
    Project {
        slug: "skyline-etl",
        name: "Skyline",
        repo: "codeberg.org/fabrikam-data/skyline-etl",
        dir: r"C:\dev\skyline-etl",
        lang: Lang::Python,
        files: [
            r"etl\extract.py",
            r"etl\transform.py",
            r"dags\nightly_load.py",
            r"tests\test_transform.py",
        ],
        tickets: [
            t("SKY-45", "Nightly load double counts late events"),
            t("SKY-52", "Backfill the August partitions"),
            t("#219", "Parametrize the partition window"),
        ],
        docs: [
            "data-contract-events.md",
            "skyline-dashboard-spec.docx",
            "late-events-analysis.csv",
        ],
        site: "skyline.fabrikam.io",
        site_paths: ["/dags/nightly_load/grid", "/datasets/events_daily"],
        tracker: Tracker::Linear("fabrikam"),
        channel: "data-eng",
        team: [5, 7, 4],
        vocab: ["event", "partition", "window", "load"],
        service: "skyline-scheduler",
    },
    Project {
        slug: "handbook",
        name: "Engineering Handbook",
        repo: "gitlab.com/northwind-labs/handbook",
        dir: r"C:\dev\handbook",
        lang: Lang::Docs,
        files: [
            r"content\engineering\oncall.md",
            r"content\engineering\code-review.md",
            r"config\site.toml",
            r"scripts\build_index.py",
        ],
        tickets: [
            t("HB-12", "Update the on-call escalation matrix"),
            t("HB-17", "Fix broken links in the code review guide"),
            t("#108", "Add the incident review template"),
        ],
        docs: [
            "engineering-ladder-2026.pdf",
            "handbook-style-guide.docx",
            "incident-review-template.md",
        ],
        site: "handbook.northwind.dev",
        site_paths: ["/engineering/oncall", "/engineering/code-review"],
        tracker: Tracker::Jira("northwind.atlassian.net"),
        channel: "handbook",
        team: [7, 0, 9],
        vocab: ["page", "section", "link", "index"],
        service: "handbook-site",
    },
    Project {
        slug: "pocketbank-android",
        name: "PocketBank",
        repo: "github.com/contoso/pocketbank-android",
        dir: r"C:\dev\pocketbank-android",
        lang: Lang::Kotlin,
        files: [
            r"app\src\main\java\com\contoso\pocketbank\LoginViewModel.kt",
            r"app\src\main\java\com\contoso\pocketbank\TransferRepository.kt",
            r"app\build.gradle.kts",
            r"app\src\test\java\com\contoso\pocketbank\TransferRepositoryTest.kt",
        ],
        tickets: [
            t("PB-930", "Biometric prompt loops after the user cancels"),
            t("PB-944", "Transfer limit copy is outdated"),
            t("#2051", "Bump compileSdk to 35"),
        ],
        docs: [
            "release-notes-5-12.md",
            "biometric-flow.pdf",
            "play-console-crashes.csv",
        ],
        site: "pocketbank.contoso.io",
        site_paths: ["/release/5.12", "/support/biometrics"],
        tracker: Tracker::Jira("contoso.atlassian.net"),
        channel: "pocketbank-mobile",
        team: [8, 1, 5],
        vocab: ["transfer", "login", "limit", "biometric"],
        service: "pocketbank-gateway",
    },
    Project {
        slug: "terraform-live",
        name: "Contoso Infra",
        repo: "gitlab.com/contoso/terraform-live",
        dir: r"C:\dev\terraform-live",
        lang: Lang::Terraform,
        files: [
            r"envs\prod\main.tf",
            r"modules\vpc\variables.tf",
            r"modules\eks\node_groups.tf",
            r"envs\staging\terraform.tfvars",
        ],
        tickets: [
            t("OPS-1187", "Rotate the NAT gateway elastic IPs"),
            t(
                "OPS-1203",
                "Staging node group is too small for the load test",
            ),
            t("#341", "Pin the AWS provider to 5.x"),
        ],
        docs: [
            "network-diagram-v4.pdf",
            "cost-report-sept.xlsx",
            "change-request-0917.docx",
        ],
        site: "infra.contoso.io",
        site_paths: ["/runbooks/nat-rotation", "/environments/staging"],
        tracker: Tracker::Jira("contoso.atlassian.net"),
        channel: "infra",
        team: [9, 3, 6],
        vocab: ["subnet", "node", "gateway", "cluster"],
        service: "staging-eks",
    },
    Project {
        slug: "inkwell",
        name: "Inkwell",
        repo: "codeberg.org/inkwell/inkwell",
        dir: r"C:\dev\inkwell",
        lang: Lang::TypeScript,
        files: [
            r"packages\core\src\schema.ts",
            r"packages\core\src\history.ts",
            r"packages\react\src\Editor.tsx",
            r"packages\core\test\history.test.ts",
        ],
        tickets: [
            t("INK-23", "Undo merges unrelated transactions"),
            t("INK-31", "Paste from Word loses list nesting"),
            t("#144", "Add a plugin API for slash commands"),
        ],
        docs: [
            "collaborative-editing-rfc.md",
            "inkwell-roadmap.pptx",
            "plugin-api.md",
        ],
        site: "inkwell.dev",
        site_paths: ["/docs/plugins", "/playground"],
        tracker: Tracker::Linear("inkwell"),
        channel: "inkwell",
        team: [2, 9, 0],
        vocab: ["transaction", "node", "history", "schema"],
        service: "inkwell-playground",
    },
    Project {
        slug: "metricsd",
        name: "metricsd",
        repo: "github.com/northwind-labs/metricsd",
        dir: r"C:\dev\metricsd",
        lang: Lang::Go,
        files: [
            r"cmd\metricsd\main.go",
            r"pkg\aggregate\window.go",
            r"pkg\aggregate\window_test.go",
            r"pkg\export\prometheus.go",
        ],
        tickets: [
            t("MET-66", "p99 window drifts after clock skew"),
            t("MET-70", "Export histogram buckets"),
            t("#905", "Reduce allocations in window merge"),
        ],
        docs: [
            "slo-definitions.md",
            "capacity-plan-2027.xlsx",
            "metricsd-architecture.pdf",
        ],
        site: "metrics.northwind.dev",
        site_paths: ["/docs/aggregation", "/docs/exporters"],
        tracker: Tracker::Jira("northwind.atlassian.net"),
        channel: "observability",
        team: [6, 3, 1],
        vocab: ["window", "sample", "bucket", "series"],
        service: "metricsd",
    },
];

/// A public reference page: the URL, its tab title and a few lines of its body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Resource {
    pub url: &'static str,
    pub title: &'static str,
    pub lines: [&'static str; 4],
    /// The language this page is usually read for (`None`: any project).
    pub lang: Option<Lang>,
}

impl Resource {
    pub(crate) fn domain(&self) -> &'static str {
        domain_of(self.url)
    }
}

/// Reference pages read while researching. Each one carries a distinct domain.
pub(crate) const RESOURCES: [Resource; 16] = [
    Resource {
        url: "https://docs.rs/tokio/latest/tokio/time/fn.sleep.html",
        title: "sleep in tokio::time - Rust",
        lines: [
            "Function sleep",
            "pub fn sleep(duration: Duration) -> Sleep",
            "Waits until duration has elapsed. Equivalent to sleep_until(Instant::now() + duration).",
            "No work is performed while awaiting on the sleep future to complete.",
        ],
        lang: Some(Lang::Rust),
    },
    Resource {
        url: "https://crates.io/crates/backoff",
        title: "backoff - crates.io: Rust Package Registry",
        lines: [
            "backoff v0.4.0",
            "Retry operations with exponential backoff policy.",
            "Install: cargo add backoff",
            "Repository and documentation linked from the crate page",
        ],
        lang: Some(Lang::Rust),
    },
    Resource {
        url: "https://stackoverflow.com/questions/77012345/how-to-add-jitter-to-exponential-backoff",
        title: "How to add jitter to exponential backoff - Stack Overflow",
        lines: [
            "How to add jitter to exponential backoff?",
            "Asked 2 years ago Modified 6 months ago Viewed 18k times",
            "Full jitter: sleep = random_between(0, min(cap, base * 2 ** attempt))",
            "Decorrelated jitter keeps retries spread out when many clients fail at once.",
        ],
        lang: None,
    },
    Resource {
        url: "https://developer.mozilla.org/en-US/docs/Web/API/AbortController",
        title: "AbortController - Web APIs | MDN",
        lines: [
            "AbortController",
            "The AbortController interface represents a controller object that allows you to abort web requests.",
            "AbortController.abort() aborts an asynchronous operation before it has completed.",
            "Baseline Widely available",
        ],
        lang: Some(Lang::TypeScript),
    },
    Resource {
        url: "https://learn.microsoft.com/en-us/dotnet/api/system.math.round",
        title: "Math.Round Method (System) | Microsoft Learn",
        lines: [
            "Math.Round Method",
            "Rounds a value to the nearest integer or to the specified number of fractional digits.",
            "MidpointRounding.ToEven is the default (banker's rounding).",
            "Round(Decimal, Int32, MidpointRounding)",
        ],
        lang: Some(Lang::CSharp),
    },
    Resource {
        url: "https://pypi.org/project/python-dateutil/",
        title: "python-dateutil - PyPI",
        lines: [
            "python-dateutil 2.9.0.post0",
            "pip install python-dateutil",
            "Extensions to the standard Python datetime module",
            "Released: Mar 1, 2024",
        ],
        lang: Some(Lang::Python),
    },
    Resource {
        url: "https://npmjs.com/package/idb-keyval",
        title: "idb-keyval - npm",
        lines: [
            "idb-keyval 6.2.1 Public",
            "A super-simple-small promise-based keyval store implemented with IndexedDB",
            "npm i idb-keyval",
            "Weekly Downloads 2,114,903",
        ],
        lang: Some(Lang::TypeScript),
    },
    Resource {
        url: "https://pkg.go.dev/k8s.io/client-go/tools/cache",
        title: "cache package - k8s.io/client-go/tools/cache - Go Packages",
        lines: [
            "package cache",
            "Package cache is a client-side caching mechanism.",
            "func NewSharedIndexInformer(lw ListerWatcher, exampleObject runtime.Object, defaultEventHandlerResyncPeriod time.Duration, indexers Indexers) SharedIndexInformer",
            "Imported by: 14,502",
        ],
        lang: Some(Lang::Go),
    },
    Resource {
        url: "https://registry.terraform.io/providers/hashicorp/aws/latest/docs/resources/eip",
        title: "aws_eip | Resources | hashicorp/aws | Terraform Registry",
        lines: [
            "Resource: aws_eip",
            "Provides an Elastic IP resource.",
            "domain - Indicates if this EIP is for use in VPC (vpc).",
            "Import: terraform import aws_eip.bar eipalloc-00a10e96",
        ],
        lang: Some(Lang::Terraform),
    },
    Resource {
        url: "https://en.wikipedia.org/wiki/Exponential_backoff",
        title: "Exponential backoff - Wikipedia",
        lines: [
            "Exponential backoff",
            "Exponential backoff is an algorithm that uses feedback to multiplicatively decrease the rate of some process.",
            "Rate limiting and truncated exponential backoff",
            "From Wikipedia, the free encyclopedia",
        ],
        lang: None,
    },
    Resource {
        url: "https://play.google.com/console/developers/app/vitals/crashes",
        title: "Crashes and ANRs - Android vitals - Google Play Console",
        lines: [
            "Crashes and ANRs",
            "User-perceived crash rate 0.41% (bad behavior threshold 1.09%)",
            "java.lang.IllegalStateException in androidx.biometric.BiometricFragment",
            "Affected users in the last 28 days: 1,284",
        ],
        lang: Some(Lang::Kotlin),
    },
    Resource {
        url: "https://console.aws.amazon.com/ec2/home?region=eu-west-1#Addresses:",
        title: "Elastic IP addresses | EC2 | eu-west-1",
        lines: [
            "Elastic IP addresses (6)",
            "Allocated IPv4 address Type Allocation ID Associated instance ID",
            "Release Elastic IP addresses Associate Elastic IP address",
            "Network border group eu-west-1",
        ],
        lang: Some(Lang::Terraform),
    },
    Resource {
        url: "https://portal.azure.com/#view/Microsoft_Azure_CostManagement/Menu/~/costanalysis",
        title: "Cost analysis - Microsoft Azure",
        lines: [
            "Cost analysis",
            "Accumulated costs this month EUR 18,402.77",
            "Group by: Service name Granularity: Daily",
            "Forecast: EUR 24,915.00",
        ],
        lang: None,
    },
    Resource {
        url: "https://docs.google.com/document/d/1hT9cV2pQ8rL4mN6bX0zY3wK5jF7gD1sA/edit",
        title: "Team offsite planning - Google Docs",
        lines: [
            "Team offsite planning",
            "Dates: 22-23 October, Lyon office",
            "Agenda owners: Hugo (product), Ines (platform), Alex (reliability)",
            "Open question: do we book the train for Thursday evening?",
        ],
        lang: None,
    },
    Resource {
        url: "https://figma.com/design/Xk29PqLm7Rt3/Lumen-components",
        title: "Lumen components - Figma",
        lines: [
            "Lumen components",
            "Button / Primary / Focus",
            "Last edited by Grace Liu 2 hours ago",
            "Dev Mode: inspect focus-ring tokens",
        ],
        lang: None,
    },
    Resource {
        url: "https://notion.so/northwind/Reliability-review-notes-4f2a9c1e7b3d",
        title: "Reliability review notes",
        lines: [
            "Reliability review notes",
            "Owner: Alex Morgan Last edited: this week",
            "Error budgets, retry storms, on-call load",
            "Linked pages and decisions",
        ],
        lang: None,
    },
];

/// Observability and delivery sites every project reports to.
pub(crate) const SENTRY: &str = "northwind.sentry.io";
pub(crate) const GRAFANA: &str = "grafana.northwind.dev";
pub(crate) const CI: &str = "ci.northwind.dev";
pub(crate) const PAGERDUTY: &str = "northwind.pagerduty.com";
pub(crate) const CLAUDE: &str = "claude.ai";

/// Office documents that belong to no project (admin and team work).
#[derive(Debug, Clone, Copy)]
pub(crate) struct AdminDoc {
    pub name: &'static str,
    /// The folder it lives in (absolute Windows path).
    pub folder: &'static str,
    pub lines: [&'static str; 3],
}

pub(crate) const ADMIN_DOCS: [AdminDoc; 6] = [
    AdminDoc {
        name: "expense-report-sept.xlsx",
        folder: r"C:\Users\alex\Documents\Finance",
        lines: [
            "Date Merchant Category Amount Receipt",
            "Train Paris-Lyon offsite prep Travel EUR 112.40 yes",
            "Team lunch with Fabrikam data team Meals EUR 86.00 yes",
        ],
    },
    AdminDoc {
        name: "travel-policy-2026.pdf",
        folder: r"C:\Users\alex\Downloads",
        lines: [
            "Travel policy 2026",
            "Rail is preferred for trips under four hours.",
            "Book through the travel desk at least 14 days ahead.",
        ],
    },
    AdminDoc {
        name: "team-offsite-agenda.docx",
        folder: r"C:\Users\alex\Documents\Offsite",
        lines: [
            "Reliability team offsite agenda",
            "Day 1: incident retros, error budget policy, on-call load",
            "Day 2: roadmap, hiring plan, team charter",
        ],
    },
    AdminDoc {
        name: "benefits-guide-2027.pdf",
        folder: r"C:\Users\alex\Downloads",
        lines: [
            "Benefits guide 2027",
            "Open enrollment runs from 1 to 30 November.",
            "Changes take effect on 1 January 2027.",
        ],
    },
    AdminDoc {
        name: "q4-okrs.xlsx",
        folder: r"C:\Users\alex\Documents\OKRs",
        lines: [
            "Objective Key result Owner Confidence",
            "Cut paging load Reduce pages per week from 9 to 4 Alex 0.6",
            "Ship retry budgets Ledgerline and Orbit on budgets by Dec Ana 0.7",
        ],
    },
    AdminDoc {
        name: "all-hands-0918.pptx",
        folder: r"C:\Users\alex\Documents\Presentations",
        lines: [
            "All hands 18 September",
            "Reliability: 99.95% availability over the quarter",
            "Hiring: two platform roles open",
        ],
    },
];

/// The host of a full URL (`https://host/...`), as the harness extractor reports it.
pub(crate) fn domain_of(url: &str) -> &str {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    &rest[..end]
}

/// Every entity the generator places, by the five kinds the eval harness extracts
/// (`crates/harness/src/eval/refs.rs`). Seed-independent; each list is sorted and
/// deduplicated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntities {
    /// `<host>/<owner>/<repo>`.
    pub repositories: Vec<String>,
    /// Absolute Windows paths with a drive letter.
    pub paths: Vec<String>,
    /// Hosts that appear inside full `https://` URLs.
    pub domains: Vec<String>,
    /// `#123` and `ABC-123` references.
    pub tickets: Vec<String>,
    /// Bare document names with a document extension.
    pub documents: Vec<String>,
}

/// The fixture's entity catalog (see [`WorldEntities`]).
pub fn world_entities() -> WorldEntities {
    let mut repositories = Vec::new();
    let mut paths = Vec::new();
    let mut domains = Vec::new();
    let mut tickets = Vec::new();
    let mut documents = Vec::new();
    for p in &PROJECTS {
        repositories.push(p.repo.to_string());
        paths.push(p.dir.to_string());
        for i in 0..p.files.len() {
            paths.push(p.file_path(i));
        }
        domains.push(p.host().to_string());
        domains.push(p.site.to_string());
        domains.push(p.tracker_domain().to_string());
        tickets.extend(p.tickets.iter().map(|t| t.key.to_string()));
        documents.extend(p.docs.iter().map(|d| d.to_string()));
    }
    for r in &RESOURCES {
        domains.push(r.domain().to_string());
    }
    for d in [SENTRY, GRAFANA, CI, PAGERDUTY, CLAUDE] {
        domains.push(d.to_string());
    }
    for doc in &ADMIN_DOCS {
        documents.push(doc.name.to_string());
        paths.push(doc.folder.to_string());
    }
    for list in [
        &mut repositories,
        &mut paths,
        &mut domains,
        &mut tickets,
        &mut documents,
    ] {
        list.sort();
        list.dedup();
    }
    WorldEntities {
        repositories,
        paths,
        domains,
        tickets,
        documents,
    }
}
