---
title: Uniswap V3
full_width: true
---

```sql sample
select * from local_duckdb.uniswap_v3_pool_swap limit 100
```

This dashboard was scaffolded by tiders-x402-server with command `tiders-x402-server dashboard`.

<DataTable data={sample} rows=10 />

<TidersDownloadButton
  label="Download sample"
  filename="uniswap_v3_pool_swap.csv"
  query={`select * from uniswap_v3_pool_swap limit 1`}
/>
