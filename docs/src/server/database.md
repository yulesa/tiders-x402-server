# Database

The database module (`server/src/database.rs`) handles all direct interaction with DuckDB. It provides three utility functions that the query handler uses to execute queries and format results.

## Functions

- **`execute_query`** — runs a SQL query against DuckDB and returns the results as Arrow record batches. This is the main path for both free and paid queries.

- **`execute_row_count_query`** — runs a `COUNT(*)` query and returns the result as a single number. Used during the estimation step to determine the price before payment.

- **`serialize_batches_to_arrow_ipc`** — converts Arrow record batches into the Arrow IPC streaming format. This is the binary format sent back to clients in successful responses.

## Why a Separate Module

These functions isolate DuckDB-specific code from the query handler. The handler deals with request parsing, payment logic, and response building — it calls into this module when it needs to talk to the database or serialize results.
