import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const size = 64;
const samples = 4;
const pixels = Buffer.alloc(size * size * 4);

function circleDistance(x, y, cx, cy) {
  return Math.hypot(x - cx, y - cy);
}

function sampleAlpha(x, y) {
  const distance = circleDistance(x, y, 32, 32);
  let alpha = Math.abs(distance - 23) <= 3.5 ? 0.42 : 0;

  const angle = Math.atan2(y - 32, x - 32);
  const onArc = angle >= -Math.PI / 2 && angle <= -0.34;
  const arcStartX = 32;
  const arcStartY = 9;
  const arcEndX = 32 + 23 * Math.cos(-0.34);
  const arcEndY = 32 + 23 * Math.sin(-0.34);
  const onRoundCap =
    circleDistance(x, y, arcStartX, arcStartY) <= 3.5 ||
    circleDistance(x, y, arcEndX, arcEndY) <= 3.5;

  if ((Math.abs(distance - 23) <= 3.5 && onArc) || onRoundCap) {
    alpha = 1;
  }
  if (distance <= 5.2) {
    alpha = 1;
  }
  return alpha;
}

for (let y = 0; y < size; y += 1) {
  for (let x = 0; x < size; x += 1) {
    let alpha = 0;
    for (let sy = 0; sy < samples; sy += 1) {
      for (let sx = 0; sx < samples; sx += 1) {
        alpha += sampleAlpha(
          x + (sx + 0.5) / samples,
          y + (sy + 0.5) / samples
        );
      }
    }
    const offset = (y * size + x) * 4;
    pixels[offset] = 0;
    pixels[offset + 1] = 0;
    pixels[offset + 2] = 0;
    pixels[offset + 3] = Math.round((alpha / samples ** 2) * 255);
  }
}

const crcTable = Array.from({ length: 256 }, (_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) {
    value = (value & 1) !== 0 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  return value >>> 0;
});

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const payload = Buffer.concat([typeBytes, data]);
  let crc = 0xffffffff;
  for (const byte of payload) {
    crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  const result = Buffer.alloc(data.length + 12);
  result.writeUInt32BE(data.length, 0);
  typeBytes.copy(result, 4);
  data.copy(result, 8);
  result.writeUInt32BE((crc ^ 0xffffffff) >>> 0, data.length + 8);
  return result;
}

const header = Buffer.alloc(13);
header.writeUInt32BE(size, 0);
header.writeUInt32BE(size, 4);
header[8] = 8;
header[9] = 6;

const scanlines = Buffer.alloc(size * (size * 4 + 1));
for (let y = 0; y < size; y += 1) {
  const rowOffset = y * (size * 4 + 1);
  scanlines[rowOffset] = 0;
  pixels.copy(scanlines, rowOffset + 1, y * size * 4, (y + 1) * size * 4);
}

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  pngChunk("IHDR", header),
  pngChunk("IDAT", deflateSync(scanlines, { level: 9 })),
  pngChunk("IEND", Buffer.alloc(0))
]);

const outputPath = fileURLToPath(
  new URL("../src-tauri/icons/tray-icon.png", import.meta.url)
);
writeFileSync(outputPath, png);
console.log(`Generated ${outputPath}`);
