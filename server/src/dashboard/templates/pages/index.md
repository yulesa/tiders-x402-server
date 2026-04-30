---
title: {{TITLE}}
full_width: true
---

```sql sample
select * from {{SOURCE_NAME}}.{{SEED_TABLE}} limit 100
```

This dashboard was scaffolded by tiders-x402-server with command `tiders-x402-server dashboard`.

<DataTable data={sample} rows=10 />

<TidersDownloadButton
  label="Download sample"
  filename="{{SEED_TABLE}}.csv"
  query={`select * from {{SEED_TABLE}} limit 1`}
/>
