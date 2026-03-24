# Root Handler

The root handler (`server/src/root_handler.rs`) has the functions to serve the `GET /` (Root) endpoint. It returns a plain-text response describing the server and its available data.

## Response Contents

The response includes:

1. **Usage instructions** — how to send queries and a link to the x402 protocol docs.
2. **Supported tables** — for each table in the payment configuration:
   - Table name
   - Schema (field names and data types), if available
   - Description, if provided
   - Whether payment is required
3. **SQL parser rules** — the restrictions enforced by the SQL parser:
   - Only `SELECT` statements
   - One statement per request
   - One table in the `FROM` clause
   - No `GROUP BY`, `HAVING`, `JOIN`, or subqueries
   - Only simple field names in `SELECT` (no expressions)
   - `WHERE`, `ORDER BY`, and `LIMIT` supported with restrictions

## Purpose

This endpoint is designed for both human users and AI agents to discover what data is available and how to query it before making paid requests.
