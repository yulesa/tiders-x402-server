---
title: {{NAME}}
---

```sql sample
select * from {{SOURCE_NAME}}.{{SEED_TABLE}} limit 100
```

# {{NAME}}

This dashboard was scaffolded by `tiders-x402-server dashboard {{NAME}}`.

<DataTable data={sample} rows=10 />

<PaidDownloadButton
  label="Download sample (paid)"
  filename="{{SEED_TABLE}}.csv"
  sql={`select * from {{SOURCE_NAME}}.{{SEED_TABLE}} limit 1000`}
/>
