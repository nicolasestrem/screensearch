// Verifies a rendered screenshot without a vision model: decodes the PNG and reports the luminance
// distribution, colour variety and per-band means, so "the canvas is not black" is a measurement
// rather than a claim. Zero dependencies: node:zlib does the only hard part.
import { readFileSync } from 'node:fs';
import { inflateSync } from 'node:zlib';

const file = process.argv[2];
if (!file) {
  console.error('usage: node tools/check-pixels.mjs <file.png>');
  process.exit(2);
}

const buf = readFileSync(file);
if (buf.readUInt32BE(0) !== 0x89504e47) {
  console.error('not a PNG:', file);
  process.exit(2);
}

let pos = 8;
let width = 0;
let height = 0;
let depth = 0;
let colorType = 0;
let interlace = 0;
const idat = [];
while (pos < buf.length) {
  const len = buf.readUInt32BE(pos);
  const type = buf.toString('ascii', pos + 4, pos + 8);
  const data = buf.subarray(pos + 8, pos + 8 + len);
  if (type === 'IHDR') {
    width = data.readUInt32BE(0);
    height = data.readUInt32BE(4);
    depth = data[8];
    colorType = data[9];
    interlace = data[12];
  } else if (type === 'IDAT') {
    idat.push(data);
  } else if (type === 'IEND') {
    break;
  }
  pos += 12 + len;
}

if (depth !== 8) {
  console.error('unsupported bit depth:', depth);
  process.exit(2);
}
if (interlace !== 0) {
  console.error('interlaced PNG not supported');
  process.exit(2);
}
const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : colorType === 0 ? 1 : 0;
if (!channels) {
  console.error('unsupported colour type:', colorType);
  process.exit(2);
}

const raw = inflateSync(Buffer.concat(idat));
const stride = width * channels;
const out = Buffer.alloc(height * stride);
let prev = Buffer.alloc(stride);
let rp = 0;

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

for (let y = 0; y < height; y += 1) {
  const filter = raw[rp];
  rp += 1;
  const line = raw.subarray(rp, rp + stride);
  rp += stride;
  const cur = Buffer.from(line);
  for (let x = 0; x < stride; x += 1) {
    const a = x >= channels ? cur[x - channels] : 0;
    const b = prev[x];
    const c = x >= channels ? prev[x - channels] : 0;
    if (filter === 1) cur[x] = (cur[x] + a) & 0xff;
    else if (filter === 2) cur[x] = (cur[x] + b) & 0xff;
    else if (filter === 3) cur[x] = (cur[x] + ((a + b) >> 1)) & 0xff;
    else if (filter === 4) cur[x] = (cur[x] + paeth(a, b, c)) & 0xff;
    else if (filter !== 0) {
      console.error('bad filter', filter, 'row', y);
      process.exit(2);
    }
  }
  cur.copy(out, y * stride);
  prev = cur;
}

const bands = [
  { name: 'top', from: 0, to: 1 / 3 },
  { name: 'middle', from: 1 / 3, to: 2 / 3 },
  { name: 'bottom', from: 2 / 3, to: 1 }
];
const bandStats = bands.map(() => ({ n: 0, sum: 0, sum2: 0 }));
const uniq = new Set();
let n = 0;
let sum = 0;
let sum2 = 0;
let nonBlack = 0;
let bright = 0;
const warmPixels = { n: 0 };

for (let y = 0; y < height; y += 1) {
  const bandIndex = y < height / 3 ? 0 : y < (2 * height) / 3 ? 1 : 2;
  for (let x = 0; x < width; x += 1) {
    const i = y * stride + x * channels;
    const r = out[i];
    const g = channels >= 3 ? out[i + 1] : out[i];
    const b = channels >= 3 ? out[i + 2] : out[i];
    const lum = 0.299 * r + 0.587 * g + 0.114 * b;
    n += 1;
    sum += lum;
    sum2 += lum * lum;
    if (lum > 10) nonBlack += 1;
    if (lum > 190) bright += 1;
    if (r > g + 12 && g >= b) warmPixels.n += 1;
    uniq.add(((r >> 3) << 10) | ((g >> 3) << 5) | (b >> 3));
    const bs = bandStats[bandIndex];
    bs.n += 1;
    bs.sum += lum;
    bs.sum2 += lum * lum;
  }
}

const mean = sum / n;
const sd = Math.sqrt(Math.max(0, sum2 / n - mean * mean));
const report = {
  file,
  width,
  height,
  channels,
  meanLuminance: +mean.toFixed(1),
  sdLuminance: +sd.toFixed(1),
  distinctColors: uniq.size,
  nonBlackPct: +((100 * nonBlack) / n).toFixed(1),
  brightPct: +((100 * bright) / n).toFixed(1),
  warmHuePct: +((100 * warmPixels.n) / n).toFixed(1),
  bands: bandStats.map((bs, i) => {
    const m = bs.sum / bs.n;
    return {
      band: bands[i].name,
      mean: +m.toFixed(1),
      sd: +Math.sqrt(Math.max(0, bs.sum2 / bs.n - m * m)).toFixed(1)
    };
  })
};

console.log(JSON.stringify(report, null, 1));

const failures = [];
if (report.nonBlackPct < 40) failures.push('less than 40 percent of pixels are lit (near black canvas)');
if (report.distinctColors < 200) failures.push('fewer than 200 distinct colours (flat or empty render)');
if (report.sdLuminance < 12) failures.push('luminance spread below 12 (no visible structure)');
if (failures.length) {
  console.error('FAIL');
  for (const f of failures) console.error('  - ' + f);
  process.exit(1);
}
console.log('PASS: the frame has structure, colour variety and is not black');
