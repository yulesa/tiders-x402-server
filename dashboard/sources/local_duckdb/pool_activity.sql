select
  address                                  as pool,
  count(*)                                 as swap_count,
  sum(abs(amount0))                        as total_amount0,
  sum(abs(amount1))                        as total_amount1,
  min(to_timestamp(timestamp))             as first_swap,
  max(to_timestamp(timestamp))             as last_swap
from uniswap_v3_pool_swap
group by address
order by swap_count desc
