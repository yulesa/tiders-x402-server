import { tableFromIPC } from 'apache-arrow';
import Papa from 'papaparse';

export async function arrowBytesToCsv(bytes: Uint8Array): Promise<string> {
  const table = tableFromIPC(bytes);
  const fields = table.schema.fields.map(f => f.name);
  const vectors = fields.map(name => table.getChild(name));
  const rows: unknown[][] = new Array(table.numRows);
  for (let i = 0; i < table.numRows; i++) {
    const row = new Array(fields.length);
    for (let c = 0; c < fields.length; c++) {
      const v = vectors[c]?.get(i);
      row[c] = typeof v === 'bigint' ? v.toString() : v;
    }
    rows[i] = row;
  }
  return Papa.unparse({ fields, data: rows });
}

export function triggerDownload(csv: string, filename: string) {
  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
