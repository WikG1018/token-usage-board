import zlib from "node:zlib";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(__dirname, "..", "src-tauri", "icons");
fs.mkdirSync(outDir, { recursive: true });

function crc32(buf) {
  let table = crc32.table;
  if (!table) {
    table = crc32.table = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      table[n] = c;
    }
  }
  let c = ~0;
  for (let i = 0; i < buf.length; i++) c = (c >>> 8) ^ table[(c ^ buf[i]) & 0xff];
  return ~c >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td), 0);
  return Buffer.concat([len, td, crc]);
}

function makePng(size) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const c = size / 2;
  const r = size / 2 - 2;
  const raw = Buffer.alloc(size * (size * 4 + 1));
  let p = 0;
  for (let y = 0; y < size; y++) {
    raw[p++] = 0; // filter
    for (let x = 0; x < size; x++) {
      const dx = x - c + 0.5;
      const dy = y - c + 0.5;
      const inside = Math.hypot(dx, dy) <= r;
      raw[p++] = inside ? 79 : 0;
      raw[p++] = inside ? 140 : 0;
      raw[p++] = inside ? 255 : 0;
      raw[p++] = inside ? 255 : 0;
    }
  }
  const idat = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function makeIco(sizes) {
  const images = sizes.map((s) => makePng(s));
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);
  const entries = [];
  let offset = 6 + sizes.length * 16;
  sizes.forEach((s, i) => {
    const e = Buffer.alloc(16);
    e[0] = s >= 256 ? 0 : s;
    e[1] = s >= 256 ? 0 : s;
    e[2] = 0;
    e[3] = 0;
    e.writeUInt16LE(1, 4);
    e.writeUInt16LE(32, 6);
    e.writeUInt32LE(images[i].length, 8);
    e.writeUInt32LE(offset, 12);
    offset += images[i].length;
    entries.push(e);
  });
  return Buffer.concat([header, ...entries, ...images]);
}

fs.writeFileSync(path.join(outDir, "32x32.png"), makePng(32));
fs.writeFileSync(path.join(outDir, "128x128.png"), makePng(128));
fs.writeFileSync(path.join(outDir, "128x128@2x.png"), makePng(256));
fs.writeFileSync(path.join(outDir, "icon.ico"), makeIco([16, 32, 48, 64, 128, 256]));
// icon.icns for macOS bundle target; on Windows it is not required but config references it.
// Provide a minimal placeholder by copying the ico bytes is invalid; instead write icns from png.
function makeIcns() {
  const png256 = makePng(256);
  const type = Buffer.from("ic09", "ascii"); // 256x256 png
  const len = Buffer.alloc(4);
  len.writeUInt32BE(png256.length + 8, 0);
  const body = Buffer.concat([type, len, png256]);
  const head = Buffer.from("icns", "ascii");
  const total = Buffer.alloc(4);
  total.writeUInt32BE(body.length + 8, 0);
  return Buffer.concat([head, total, body]);
}
fs.writeFileSync(path.join(outDir, "icon.icns"), makeIcns());

console.log("icons written to", outDir);
