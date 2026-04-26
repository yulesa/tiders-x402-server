select
  date_trunc('day', to_timestamp(timestamp)) as day,
  count(*)                                   as swap_count
from uniswap_v3_pool_swap
group by 1
order by 1
