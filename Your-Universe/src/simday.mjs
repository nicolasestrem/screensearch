// Localtime simulated day.
//
// Pure and deterministic: no DOM, no THREE, no Date.now, no Math.random.
// Everything below lives inside the factory closure so the assembler can merge
// this fragment with the others into one module scope.
//
// The day is 2026-11-05 in Europe/Paris (CET, UTC+1). Frames run from 08:00 to
// 23:00 local. The shape of the day is deliberate:
//   - one 3-hour-plus focus block on a single app with every gap under 90 s
//   - one fragmented local hour holding exactly 40 app switches
//   - two apps opened exactly once for under a minute each
//   - Figma and Outlook share exactly three salient words and never share an hour
//   - window titles recycled across hours (tram stops)
//   - two phrases repeated in two apps within ten minutes (parcels)

export function generateSimulatedDay(seed = 20261105) {
  const TZ_OFFSET_MS = 3600000;
  const DAY_UTC0 = Date.UTC(2026, 10, 5, 0, 0, 0);
  const MINUTE = 60000;
  const HOUR = 3600000;

  function mulberry32(state) {
    let s = state >>> 0;
    return function random() {
      s = (s + 0x6d2b79f5) >>> 0;
      let t = s;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  const rng = mulberry32(seed);

  // Local wall time on 2026-11-05 expressed as UTC epoch ms.
  function at(hour, minute, second) {
    return DAY_UTC0 + hour * HOUR + minute * MINUTE + (second || 0) * 1000 - TZ_OFFSET_MS;
  }

  // Sentence banks are per app on purpose: apart from the three words shared by
  // Figma and Outlook, and the two parcel phrases, no content word of five or
  // more letters crosses an app boundary.
  const APPS = {
    vscode: {
      app: 'VS Code',
      titles: ['screensearch', 'app.mjs', 'pipeline.mjs', 'analysis.rs', 'build.log', 'district.mjs'],
      noise: ['src/app.mjs:412  68  Saved', 'Problems 0   Warnings 2', 'UTF-8  LF  JavaScript', 'Ln 184, Col 7'],
      lines: [
        'The frustum test culls most fence quads',
        'Instancing keeps the draw calls under forty',
        'Gutter marks warnings on line forty two',
        'Shader compile lands under twenty milliseconds',
        'Raycast maps the click onto the atlas',
        'Profile capture reports steady frame timings',
        'Console prints the vertex buffer size',
        'Refactor extracts helpers from main',
        'Backlog tracks the missing fence texture'
      ]
    },
    firefox: {
      app: 'Firefox',
      titles: ['Wayland resume fails - search', 'Parcel tracking - search', 'GDAL docs - projection', 'Kernel patches - forum'],
      noise: ['Tab 4 of 9', 'Reader mode on', '1 tab muted', 'Downloads 12'],
      lines: [
        'Wayland resume fails on the second monitor',
        'Parcel tracking shows the van left the depot',
        'Gdal docs explain the projection maths',
        'Search results point at a suspend workaround',
        'Forum thread lists three kernel patches',
        'Reader view strips the banner and the footer',
        'Bookmark folder saves the older snapshots',
        'Extension page warns about the pending patch',
        'Tab group collapses after a long pause'
      ]
    },
    slack: {
      app: 'Slack',
      titles: ['#engineering - Slack', '#design-crit - Slack', '#general - Slack', 'Ana Duarte - Slack'],
      noise: ['#engineering 128 replies', 'Today at 14:12', 'Reply in channel', '4 replies'],
      lines: [
        'Standup starts with sprint blockers',
        'Could you check the staging deploy',
        'I pushed the fix onto main branch',
        'Retro doc updated with two blockers',
        'Office wifi dropped during the call',
        'Approved the holiday request from Ana',
        'Channel archive holds the early decisions',
        'Designer posted a new poster draft',
        'Morning check about the office paint'
      ]
    },
    figma: {
      app: 'Figma',
      titles: ['localtime-hillside-v3', 'warehouse-dashboard', 'invoice-table-redesign', 'quarterly-review-copy'],
      noise: ['Canvas 412 x 268', 'Zoom 82 percent', 'Auto layout 12 px', 'Fill 100 percent'],
      lines: [
        'Warehouse dashboard frames need tighter spacing',
        'Invoice table uses the warm palette',
        'Quarterly review layout waits on copy',
        'Sidebar variants collapse into one component',
        'Prototype links every warehouse row',
        'Export the invoice mockup at retina',
        'Quarterly numbers sit in the hero card',
        'Warm palette stays at the lower layers',
        'Guides hold the hero card at eight pixels'
      ]
    },
    obsidian: {
      app: 'Obsidian',
      titles: ['daily notes', 'reading list', 'project log', 'style guide'],
      noise: ['[[daily]] #reading', 'Word count 412', 'Vault 68 notes', 'Backlinks 12'],
      lines: [
        'Daily note logs the afternoon reading',
        'Reading list grows with two essays',
        'Quotes from the paper on habit',
        'Headings stay short in the vault',
        'Bullets collect stray thoughts',
        'Release checklist not signed',
        'Weekly digest drafts the next entry',
        'Margin notes mark the long paragraphs',
        'Journal pages collect the still moments'
      ]
    },
    outlook: {
      app: 'Outlook',
      titles: ['Inbox - 12 unread', 'Re: quarterly plan', 'Supplier schedule', 'Budget sheet 2027'],
      noise: ['Inbox 12 unread', 'Re: quarterly plan', 'Sent Tue 09:14', 'Outbox 3'],
      lines: [
        'Inbox carries twelve unread messages',
        'Quarterly invoice attached from the supplier',
        'Forwarded the warehouse schedule to Ana',
        'Supplier confirms pickup window tomorrow',
        'Approval needed before the deadline',
        'Meeting minutes mention the budget line',
        'Budget sheet covers the winter quarter',
        'Subject line reads urgent on the memo',
        'Attached the lease scan from facilities'
      ]
    },
    spotify: {
      app: 'Spotify',
      titles: ['Liked Songs', 'Deep Work mix'],
      noise: ['3 songs 12:04', 'Shuffle on', 'Lossless 44 kHz'],
      lines: [
        'Liked Songs playlist on shuffle',
        'Volume sits near the top',
        'Cover art for the slow album',
        'A long mix of ambient piano',
        'Queued for later listening',
        'Skip straight to the chorus',
        'Playlist ends with a quiet fade',
        'Headphones on for the deep work block',
        'Shuffle turns the album around'
      ]
    },
    excel: {
      app: 'Excel',
      titles: ['budget-2026.xlsx', 'Q4 warehouse costs.xlsx'],
      noise: ['Sheet1  cell C12', 'SUM 1,204', 'Formula bar', 'Row 42 of 120'],
      lines: [
        'Worksheet formula fills the total column',
        'Pivot grid counts rows per region',
        'Column headers carry the month labels',
        'Totals refresh once the last paste ends',
        'Formula bar gives sum across the range',
        'Tabs stack at the bottom edge',
        'Freeze panes keep the header visible',
        'Conditional colors flag the low cells',
        'Print area set to the long side'
      ]
    }
  };

  function wordCount(line) {
    let count = 0;
    let inWord = false;
    for (let i = 0; i < line.length; i++) {
      const code = line.charCodeAt(i);
      if (code === 32 || code === 10 || code === 9) {
        inWord = false;
      } else if (!inWord) {
        inWord = true;
        count++;
      }
    }
    return count;
  }

  // Nine lines per bank and a stride of two keep one frame free of repeats.
  // The clamp at 72 words leaves room for the noise line.
  function textFor(key, salt, extra) {
    const def = APPS[key];
    const lines = [];
    let words = 0;
    if (extra) {
      lines.push(extra);
      words += wordCount(extra);
    }
    const pick = 3 + (salt % 3);
    for (let i = 0; i < pick; i++) {
      const line = def.lines[(salt + i * 2) % def.lines.length];
      lines.push(line);
      words += wordCount(line);
    }
    for (let k = pick; words < 26 && k < pick + 4; k++) {
      const line = def.lines[(salt + k * 2) % def.lines.length];
      lines.push(line);
      words += wordCount(line);
    }
    while (words > 72 && lines.length > 1) {
      words -= wordCount(lines.pop());
    }
    lines.push(def.noise[(salt + pick) % def.noise.length]);
    return lines.join('\n');
  }

  function urlFor(key, title) {
    if (key !== 'firefox') return null;
    if (title.indexOf('Wayland') === 0) return 'https://forum.example/wayland-resume';
    if (title.indexOf('Parcel') === 0) return 'https://parcels.example/tracking';
    return null;
  }

  const records = [];

  function addFrame(t, key, title) {
    records.push({
      t: Math.round(t),
      key: key,
      title: title,
      salt: records.length,
      extra: null
    });
  }

  // Captures are gated on screen changes, so one window held for hours yields a
  // frame every minute or so, never a continuous stream. `steadyMin` and
  // `steadyMax` are seconds and stay under the 90 s session split.
  function runSteady(key, startMs, endMs, steadyMin, steadyMax, titleOffset) {
    const def = APPS[key];
    let t = startMs;
    let i = 0;
    while (t <= endMs) {
      addFrame(t, key, def.titles[(titleOffset + i) % def.titles.length]);
      i++;
      const seconds = steadyMin + Math.floor(rng() * (steadyMax - steadyMin));
      t += seconds * 1000;
    }
  }

  // Everywhere else the screen changes in bursts: a few frames a second apart
  // while clicking and scrolling, then minutes of quiet.
  function runBursty(key, startMs, endMs, titleOffset) {
    const def = APPS[key];
    let t = startMs;
    let n = 0;
    while (t <= endMs) {
      const size = 3 + Math.floor(rng() * 6);
      for (let b = 0; b < size; b++) {
        if (t > endMs) break;
        addFrame(t, key, def.titles[(titleOffset + n) % def.titles.length]);
        n++;
        t += 2000 + Math.floor(rng() * 12000);
      }
      t += 150000 + Math.floor(rng() * 260000);
    }
  }

  // Two frames, one short session: the app is opened once, briefly. These are
  // placed in windows where no other segment runs, so nothing splits them.
  function runPair(key, startMs, gapMs, titleOffset) {
    const def = APPS[key];
    addFrame(startMs, key, def.titles[titleOffset % def.titles.length]);
    addFrame(startMs + gapMs, key, def.titles[(titleOffset + 1) % def.titles.length]);
  }

  // ---------------------------------------------------------------- morning
  runBursty('outlook', at(8, 0, 0), at(8, 9, 30), 1);
  runBursty('slack', at(8, 10, 20), at(8, 13, 10), 2);
  runBursty('outlook', at(8, 15, 30), at(8, 17, 10), 3);
  runPair('excel', at(8, 18, 0), 45000, 0);
  runBursty('outlook', at(8, 20, 30), at(8, 25, 30), 4);

  // ------------------------------------------------- the focus block, 3h+
  runSteady('vscode', at(8, 26, 20), at(11, 41, 0), 60, 88, 5);

  // ------------------------------------------------------------ lunch break
  runBursty('firefox', at(12, 44, 0), at(12, 47, 0), 6);
  addFrame(at(12, 58, 30), 'slack', APPS.slack.titles[1]);

  // ------------------------------------------- the fragmented hour, 40 switches
  // A frantic ten minutes of alt-tabbing first: 24 switches inside the 13:00
  // window, so switchRateAt crosses its stop-and-go threshold there. The rest of
  // the hour follows in shorter bursts, for 16 more switches and 40 in total.
  const FRAG_CYCLE = ['slack', 'firefox', 'vscode', 'obsidian'];
  let fragT = at(13, 0, 30);
  for (let i = 0; i < 25; i++) {
    const key = FRAG_CYCLE[i % FRAG_CYCLE.length];
    addFrame(fragT, key, APPS[key].titles[(i + 2) % APPS[key].titles.length]);
    fragT += 22000;
  }
  const fragTail = [];
  for (let i = 0; i < 16; i++) fragTail.push(FRAG_CYCLE[(i + 1) % FRAG_CYCLE.length]);
  // Three sticky repeats: 19 frames, 18 transitions, 15 switches, and with the
  // bridge out of the frantic stretch the tail holds 16 of the 40.
  const fragSticky = [4, 9, 14];
  for (let k = fragSticky.length - 1; k >= 0; k--) {
    const idx = fragSticky[k];
    fragTail.splice(idx + 1, 0, fragTail[idx]);
  }
  let tailT = at(13, 11, 30);
  for (let i = 0; i < fragTail.length; i++) {
    const key = fragTail[i];
    addFrame(tailT, key, APPS[key].titles[(i + 1) % APPS[key].titles.length]);
    tailT += 135000;
  }

  // ------------------------------------------------------------- afternoon
  runBursty('vscode', at(14, 0, 0), at(15, 2, 0), 10);
  runBursty('slack', at(15, 4, 0), at(15, 12, 20), 11);
  runBursty('firefox', at(15, 17, 40), at(15, 22, 40), 12);
  runBursty('slack', at(15, 24, 30), at(15, 30, 40), 13);
  runBursty('vscode', at(15, 33, 0), at(16, 15, 0), 14);
  runBursty('figma', at(16, 20, 0), at(17, 18, 0), 15);
  runBursty('obsidian', at(17, 21, 0), at(17, 24, 30), 16);
  runPair('spotify', at(17, 26, 0), 40000, 0);
  runBursty('obsidian', at(17, 28, 0), at(17, 55, 0), 17);

  // ---------------------------------------------------------------- evening
  runBursty('obsidian', at(19, 35, 0), at(19, 53, 0), 18);
  runBursty('vscode', at(19, 54, 30), at(19, 58, 30), 19);
  runBursty('slack', at(20, 0, 30), at(20, 24, 0), 20);
  runBursty('firefox', at(20, 26, 0), at(20, 52, 0), 21);
  runBursty('vscode', at(20, 55, 0), at(21, 18, 0), 22);
  runBursty('outlook', at(21, 48, 0), at(22, 4, 0), 23);
  runBursty('obsidian', at(22, 20, 0), at(22, 56, 0), 24);

  // ---------------------------------------------------------------- parcels
  // The same phrase inside two apps, minutes apart. The frame is dropped into a
  // window where that app is already in use, so no timestamp is invented.
  function addParcel(key, t, phrase, titleOffset) {
    records.push({
      t: Math.round(t),
      key: key,
      title: APPS[key].titles[titleOffset % APPS[key].titles.length],
      salt: records.length,
      extra: phrase
    });
  }

  addParcel('firefox', at(15, 20, 30), 'Delivery slot moved again', 3);
  addParcel('slack', at(15, 26, 10), 'Delivery slot moved again', 2);
  addParcel('obsidian', at(19, 50, 0), 'Release checklist not signed', 0);
  addParcel('vscode', at(19, 56, 40), 'Release checklist not signed', 4);

  // ---------------------------------------------------------------- assemble
  records.sort(function byTime(a, b) {
    return a.t - b.t;
  });

  const frames = [];
  for (let i = 0; i < records.length; i++) {
    const rec = records[i];
    frames.push({
      frame_id: i + 1,
      captured_at: rec.t,
      app_hint: APPS[rec.key].app,
      window_title: rec.title,
      browser_url: urlFor(rec.key, rec.title),
      activity_type: null,
      content_text: textFor(rec.key, rec.salt, rec.extra)
    });
  }

  return {
    schema: 'screensearch.export.v1',
    exported_at: at(23, 30, 0),
    from: null,
    to: null,
    frames: frames,
    marks: []
  };
}
