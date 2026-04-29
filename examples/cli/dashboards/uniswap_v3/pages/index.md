---
title: Uniswap V3
full_width: true
---

```sql sample
select * from local_duckdb.uniswap_v3_pool_swap limit 10
```

This dashboard was scaffolded by `tiders-x402-server dashboard uniswap_v3`.

<DataTable data={sample} rows=10 />

<PaidDownloadButton
  label="Download"
  filename="uniswap_v3_pool_swap.csv"
  sql={`select * from local_duckdb.uniswap_v3_pool_swap limit 1000`}
/>
