select
  to_timestamp(timestamp) as block_time,
  address                 as pool,
  sender,
  recipient,
  amount0,
  amount1,
  tick,
  block_number,
  transaction_hash
from uniswap_v3_pool_swap
order by timestamp desc
