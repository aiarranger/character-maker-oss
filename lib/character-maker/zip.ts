// ZIP を作る（three・fflate を読まない）。中身はブラウザの CompressionStream（deflate-raw）で縮める。level: 0 は縮めずに入れる。
const table = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; }
  return t;
})();
function crc32(data: Uint8Array) {
  let c = 0xffffffff;
  for (let i = 0; i < data.length; i++) c = table[(c ^ data[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
async function deflate(data: Uint8Array) {
  const stream = new Blob([data.slice()]).stream().pipeThrough(new CompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

export type ZipEntry = Uint8Array | [Uint8Array, { level: 0 }];

/** fflate の zipSync と同じ形の入力（名前 → 中身）で ZIP を作る */
export async function zipFiles(files: Record<string, ZipEntry>): Promise<Uint8Array> {
  const encoder = new TextEncoder();
  const parts: Uint8Array[] = [], central: Uint8Array[] = [];
  let offset = 0;
  for (const [name, entry] of Object.entries(files)) {
    const data = Array.isArray(entry) ? entry[0] : entry;
    const store = Array.isArray(entry) && entry[1].level === 0;
    const body = store ? data : await deflate(data);
    const method = store ? 0 : 8, crc = crc32(data), nameBytes = encoder.encode(name);
    const local = new Uint8Array(30 + nameBytes.length), lv = new DataView(local.buffer);
    lv.setUint32(0, 0x04034b50, true); lv.setUint16(4, 20, true); lv.setUint16(6, 0x0800, true); lv.setUint16(8, method, true);
    lv.setUint32(14, crc, true); lv.setUint32(18, body.length, true); lv.setUint32(22, data.length, true); lv.setUint16(26, nameBytes.length, true);
    local.set(nameBytes, 30);
    const head = new Uint8Array(46 + nameBytes.length), cv = new DataView(head.buffer);
    cv.setUint32(0, 0x02014b50, true); cv.setUint16(4, 20, true); cv.setUint16(6, 20, true); cv.setUint16(8, 0x0800, true); cv.setUint16(10, method, true);
    cv.setUint32(16, crc, true); cv.setUint32(20, body.length, true); cv.setUint32(24, data.length, true); cv.setUint16(28, nameBytes.length, true);
    cv.setUint32(42, offset, true);
    head.set(nameBytes, 46);
    parts.push(local, body); central.push(head);
    offset += local.length + body.length;
  }
  const centralSize = central.reduce((sum, item) => sum + item.length, 0);
  const end = new Uint8Array(22), ev = new DataView(end.buffer);
  ev.setUint32(0, 0x06054b50, true); ev.setUint16(8, central.length, true); ev.setUint16(10, central.length, true);
  ev.setUint32(12, centralSize, true); ev.setUint32(16, offset, true);
  const out = new Uint8Array(offset + centralSize + 22);
  let at = 0;
  for (const part of [...parts, ...central, end]) { out.set(part, at); at += part.length; }
  return out;
}
