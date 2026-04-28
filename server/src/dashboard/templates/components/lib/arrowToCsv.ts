import { tableFromIPC } from 'apache-arrow';
import Papa from 'papaparse';

export async function arrowBytesToCsv(bytes: Uint8Array): Promise<string> {
  const table = tableFromIPC(bytes);
  const columns = table.schema.fields.map(f => f.name);
  const rows: Record<string, unknown>[] = [];
  for (let i = 0; i < table.numRows; i++) {
    const row: Record<string, unknown> = {};
    for (const col of columns) {
      const v = table.getChild(col)?.get(i);
      row[col] = typeof v === 'bigint' ? v.toString() : v;
    }
    rows.push(row);
  }
  return Papa.unparse({ fields: columns, data: rows.map(r => columns.map(c => r[c])) });
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
