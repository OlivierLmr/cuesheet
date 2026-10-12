// Draw a document with the browser build of the layout, outside a browser:
//   node tools/wasm-draw.js <cuesheet_wasm.wasm> <doc.cuesheet> <sheet.cuestyle>
// Used by build-wasm.sh to compare what it draws with what the command line draws.
const fs = require('fs');
const [, , wasm, doc, style] = process.argv;
WebAssembly.instantiate(fs.readFileSync(wasm), {}).then(({ instance }) => {
  const x = instance.exports;
  const put = path => {
    const b = fs.readFileSync(path), p = x.room(b.length);
    new Uint8Array(x.memory.buffer, p, b.length).set(b);
    return [p, b.length];
  };
  // -1: the axis the sheet says. 1: the scale the sheet says.
  const head = x.draw(...put(doc), ...put(style), -1, 1);
  const view = new DataView(x.memory.buffer);
  const out = Buffer.from(new Uint8Array(x.memory.buffer, view.getUint32(head, true), view.getUint32(head + 4, true))).toString();
  if (out[0] !== '+') { console.error(out.slice(1)); process.exit(1); }
  process.stdout.write(out.slice(1));
});
