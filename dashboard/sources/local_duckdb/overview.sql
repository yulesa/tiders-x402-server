select
  count(*)                   as total_swaps,
  count(distinct address)    as distinct_pools,
  count(distinct sender)     as distinct_senders,
  min(to_timestamp(timestamp)) as first_swap,
  max(to_timestamp(timestamp)) as last_swap
from uniswap_v3_pool_swap
