---
title: Tiders — Uniswap V3 Swaps
og:
  title: Tiders — Uniswap V3 Swaps Storefront
  description: Pay-per-query access to on-chain Uniswap V3 swap data via x402.
---

```sql overview
select * from local_duckdb.overview
```

## Dataset at a glance

<Grid cols=4>
  <BigValue
    data={overview}
    value=total_swaps
    title="Total swaps"
    fmt=num0
  />
  <BigValue
    data={overview}
    value=distinct_pools
    title="Pools tracked"
    fmt=num0
  />
  <BigValue
    data={overview}
    value=distinct_senders
    title="Unique senders"
    fmt=num0
  />
  <BigValue
    data={overview}
    value=last_swap
    title="Last swap"
    fmt="mmm d, h:mm am/pm"
  />
</Grid>

<Alert status="info">
  Coverage window: <Value data={overview} column=first_swap fmt="mmm d, yyyy"/> → <Value data={overview} column=last_swap fmt="mmm d, yyyy"/>. Data is refreshed at build time; see the API for live reads.
</Alert>

---

## Activity over time

```sql daily_swaps_page
select day, swap_count
from local_duckdb.daily_swaps
order by day
```

<LineChart
  data={daily_swaps_page}
  x=day
  y=swap_count
  title="Daily swap count"
  subtitle="Across all tracked pools"
  chartAreaHeight={320}
  yAxisTitle="Swaps"
  markers=true
  yMin=0
/>

---

## Explore by pool

```sql pools
select pool as value, pool as label
from local_duckdb.pool_activity
order by swap_count desc
```

<Dropdown data={pools} name=pool value=value label=label title="Filter by pool">
  <DropdownOption value="%" valueLabel="All pools" />
</Dropdown>

```sql filtered
select
  date_trunc('hour', block_time) as hour,
  count(*)                       as swaps,
  sum(abs(amount0))              as volume0,
  sum(abs(amount1))              as volume1
from local_duckdb.swaps_detail
where pool like '${inputs.pool.value}'
group by 1
order by 1
```

<Grid cols=2>
  <BarChart
    data={filtered}
    x=hour
    y=swaps
    title="Swaps per hour"
    chartAreaHeight={260}
    yMin=0
  />
  <LineChart
    data={filtered}
    x=hour
    y=volume0
    title="|amount0| per hour"
    chartAreaHeight={260}
  />
</Grid>

---

## Top pools by activity

```sql pool_activity
select * from local_duckdb.pool_activity order by swap_count desc
```

<DataTable data={pool_activity} search=true rows=8>
  <Column id=pool title="Pool address" />
  <Column id=swap_count title="Swaps" fmt=num0 contentType=colorscale colorScale=blues />
  <Column id=total_amount0 title="Σ |amount0|" fmt=num0 />
  <Column id=total_amount1 title="Σ |amount1|" fmt=num0 />
  <Column id=first_swap title="First" fmt="mmm d, h:mm am/pm" />
  <Column id=last_swap title="Last" fmt="mmm d, h:mm am/pm" />
</DataTable>

---

## Raw swaps

Browse the underlying rows. The same data is available, unrestricted and paid,
via the x402-gated API on this server.

<div class="mb-3">
  <PaidDownloadButton
    label="Download full dataset (paid)"
    filename="uniswap_v3_pool_swap.csv"
    sql={`SELECT block_number, address, sender, recipient, amount0, amount1, tick, timestamp FROM uniswap_v3_pool_swap LIMIT 1000`}
  />
</div>

```sql swaps_detail
select * from local_duckdb.swaps_detail order by block_time desc
```

<DataTable data={swaps_detail} rows=10 search=true downloadable=true>
  <Column id=block_time title="Time" fmt="mmm d, h:mm:ss am/pm" />
  <Column id=pool title="Pool" />
  <Column id=sender title="Sender" />
  <Column id=recipient title="Recipient" />
  <Column id=amount0 fmt=num2 />
  <Column id=amount1 fmt=num0 />
  <Column id=tick fmt=num0 />
  <Column id=block_number title="Block" fmt=num0 />
</DataTable>

---

<div class="mt-8 p-4 rounded-lg bg-slate-800/50 border border-slate-700">
  <div class="text-sm uppercase tracking-widest text-blue-400 font-semibold">
    Ready for more?
  </div>
  <div class="text-lg mt-1">
    Stream the full, unsampled dataset with a single SQL query.
  </div>
  <div class="text-sm text-slate-400 mt-2">
    <code>POST /query</code> — pay per row with x402, receive Arrow IPC.
    See <code>GET /</code> for schemas and pricing.
  </div>
</div>
