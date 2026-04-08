use arrow::datatypes::Schema;
/// tiders_x402_python: Python bindings for the tiders_x402 Rust library.
///
/// This module exposes payment, server, and configuration primitives for use in Python.
///
/// Exposed classes:
/// - PriceTag: Represents a payment offer for a table or item.
/// - USDC: Represents a USDC token on a supported network.
/// - TablePaymentOffers: Holds payment offers for a table.
/// - GlobalPaymentConfig: Global configuration for payment and facilitator.
/// - AppState: Application state, including database and payment config.
/// - FacilitatorClient: Client for interacting with a payment facilitator.
/// - start_server: Start a payment-enabled server (blocking call).
/// - DuckDbDatabase: DuckDB database backend.
/// - PostgresqlDatabase: PostgreSQL database backend.
/// - ClickHouseDatabase: ClickHouse database backend.
use pyo3::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use url::Url;

#[cfg(any(feature = "duckdb", feature = "postgresql", feature = "clickhouse"))]
use ::tiders_x402_server::Database;
#[cfg(feature = "clickhouse")]
use ::tiders_x402_server::database_clickhouse::ClickHouseDatabase;
#[cfg(feature = "duckdb")]
use ::tiders_x402_server::database_duckdb::DuckDbDatabase;
#[cfg(feature = "postgresql")]
use ::tiders_x402_server::database_postgresql::PostgresqlDatabase;
use ::tiders_x402_server::price::TokenAmount;
use ::tiders_x402_server::{
    AppState, FacilitatorClient, GlobalPaymentConfig, PriceTag, PricingModel, TablePaymentOffers,
    start_server,
};
use alloy::primitives::U256;
use arrow::pyarrow::FromPyArrow;
#[cfg(any(feature = "duckdb", feature = "postgresql", feature = "clickhouse"))]
use arrow::pyarrow::ToPyArrow;
use x402_chain_eip155::KnownNetworkEip155;
use x402_chain_eip155::chain::{ChecksummedAddress, Eip155TokenDeployment};
use x402_types::networks::USDC;

/// A Python module implemented in Rust.
#[pymodule]
fn tiders_x402_server(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyPriceTag>()?;
    m.add_class::<PyUSDC>()?;
    m.add_class::<PyTablePaymentOffers>()?;
    m.add_class::<PyGlobalPaymentConfig>()?;
    m.add_class::<PyFacilitatorClient>()?;
    m.add_class::<PyAppState>()?;
    m.add_function(wrap_pyfunction!(start_server_py, m)?)?;
    #[cfg(feature = "duckdb")]
    m.add_class::<PyDuckDbDatabase>()?;
    #[cfg(feature = "postgresql")]
    m.add_class::<PyPostgresqlDatabase>()?;
    #[cfg(feature = "clickhouse")]
    m.add_class::<PyClickHouseDatabase>()?;
    Ok(())
}

/// Represents a payment offer for a table or item.
#[pyclass(name = "PriceTag", from_py_object)]
#[derive(Clone)]
pub struct PyPriceTag {
    inner: PriceTag,
}

/// Parses a token amount from a Python object (string or integer).
///
/// If a string (e.g., "0.002"), it is interpreted as a human-readable amount
/// and converted using the token's decimals. If an integer, it is treated
/// as an amount in the token's smallest unit.
fn parse_token_amount(
    obj: &Py<PyAny>,
    token: &Eip155TokenDeployment,
    param_name: &str,
    py: Python,
) -> PyResult<TokenAmount> {
    if let Ok(amount_str) = obj.extract::<String>(py) {
        let deployed_amount = token
            .parse(amount_str.as_str())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(TokenAmount(deployed_amount.amount))
    } else if let Ok(amount_int) = obj.extract::<i64>(py) {
        if amount_int < 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "{} cannot be negative",
                param_name
            )));
        }
        Ok(TokenAmount(U256::from(amount_int as u64)))
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "{} must be either a string (e.g., '0.001') or an integer representing the smallest token unit",
            param_name
        )))
    }
}

#[pymethods]
impl PyPriceTag {
    /// Create a per-row PriceTag where price scales with the number of rows returned.
    ///
    /// Args:
    ///     pay_to (str): EVM address to pay to.
    ///     amount_per_item (Union[str, int]): Amount per item (rows or tuples). If a string (e.g., "0.002") it is interpreted as a human-readable amount and converted using the token's decimals. If an integer it is the amount in the token's smallest unit.
    ///     token (USDC): Token with decimals and EIP712 information, currently only USDC is supported.
    ///     min_total_amount (Optional[Union[str, int]]): Minimum total amount for this offer (optional). Can be a string or integer.
    ///     min_items (Optional[int]): Minimum number of items for this tier (optional).
    ///     max_items (Optional[int]): Maximum number of items for this tier (optional).
    ///     description (Optional[str]): Description of the offer (optional).
    ///     is_default (bool): Whether this is the default offer.
    ///
    /// Returns:
    ///     PriceTag: A new per-row PriceTag object.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (pay_to, amount_per_item, token, min_total_amount=None, min_items=None, max_items=None, description=None, is_default=true))]
    fn new(
        pay_to: &str,
        amount_per_item: Py<PyAny>,
        token: &PyUSDC,
        min_total_amount: Option<Py<PyAny>>,
        min_items: Option<usize>,
        max_items: Option<usize>,
        description: Option<String>,
        is_default: bool,
        py: Python,
    ) -> PyResult<Self> {
        let pay_to = ChecksummedAddress::from_str(pay_to)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let token_deployment = &token.inner;
        let amount = parse_token_amount(&amount_per_item, token_deployment, "amount_per_item", py)?;
        let min_total = if let Some(min_obj) = min_total_amount {
            Some(parse_token_amount(
                &min_obj,
                token_deployment,
                "min_total_amount",
                py,
            )?)
        } else {
            None
        };

        Ok(Self {
            inner: PriceTag {
                pay_to,
                pricing: PricingModel::PerRow {
                    amount_per_item: amount,
                    min_items,
                    max_items,
                    min_total_amount: min_total,
                },
                token: token_deployment.clone(),
                description,
                is_default,
            },
        })
    }

    /// Create a fixed-price PriceTag where any query is charged the same flat fee.
    ///
    /// Args:
    ///     pay_to (str): EVM address to pay to.
    ///     fixed_amount (Union[str, int]): Fixed amount charged for any query. If a string (e.g., "1.00") it is interpreted as a human-readable amount and converted using the token's decimals. If an integer it is the amount in the token's smallest unit.
    ///     token (USDC): Token with decimals and EIP712 information, currently only USDC is supported.
    ///     description (Optional[str]): Description of the offer (optional).
    ///     is_default (bool): Whether this is the default offer.
    ///
    /// Returns:
    ///     PriceTag: A new fixed-price PriceTag object.
    #[staticmethod]
    #[pyo3(signature = (pay_to, fixed_amount, token, description=None, is_default=true))]
    fn fixed(
        pay_to: &str,
        fixed_amount: Py<PyAny>,
        token: &PyUSDC,
        description: Option<String>,
        is_default: bool,
        py: Python,
    ) -> PyResult<Self> {
        let pay_to = ChecksummedAddress::from_str(pay_to)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let token_deployment = &token.inner;
        let amount = parse_token_amount(&fixed_amount, token_deployment, "fixed_amount", py)?;

        Ok(Self {
            inner: PriceTag {
                pay_to,
                pricing: PricingModel::Fixed { amount },
                token: token_deployment.clone(),
                description,
                is_default,
            },
        })
    }
}

/// Represents a USDC token on a supported network.
#[pyclass(name = "USDC", from_py_object)]
#[derive(Clone)]
pub struct PyUSDC {
    inner: Eip155TokenDeployment,
}

#[pymethods]
impl PyUSDC {
    /// Create a USDC token for a given network.
    ///
    /// Args:
    ///     network (Optional[str]): Network name (e.g., "base_sepolia", "base", "avalanche_fuji", "avalanche", "polygon", "polygon_amoy"). Defaults to "base".
    ///
    /// Returns:
    ///     USDC: A USDC token for the specified network.
    #[new]
    #[pyo3(signature = (network=None))]
    fn new(py: Python, network: Option<Py<PyAny>>) -> PyResult<Self> {
        let network_str = match network {
            None => "base".to_string(),
            Some(net) => {
                // Accept either a string or a Python Enum value
                if let Ok(s) = net.extract::<String>(py) {
                    s
                } else if let Ok(obj) = net.getattr(py, "value") {
                    obj.extract::<String>(py)?
                } else {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "Expected a string or Network enum value",
                    ));
                }
            }
        };

        let deployment = match network_str.as_str() {
            "base_sepolia" => USDC::base_sepolia(),
            "base" => USDC::base(),
            "avalanche_fuji" => USDC::avalanche_fuji(),
            "avalanche" => USDC::avalanche(),
            "polygon" => USDC::polygon(),
            "polygon_amoy" => USDC::polygon_amoy(),
            _ => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid network: {}. Supported: base_sepolia, base, avalanche_fuji, avalanche, polygon, polygon_amoy",
                    network_str
                )));
            }
        };

        Ok(Self { inner: deployment })
    }
}

/// Holds payment offers for a table.
#[pyclass(name = "TablePaymentOffers", from_py_object)]
#[derive(Clone)]
pub struct PyTablePaymentOffers {
    inner: TablePaymentOffers,
}

#[pymethods]
impl PyTablePaymentOffers {
    /// Create a new TablePaymentOffers with a table name and price tags.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///     price_tags (List[PriceTag]): List of price tags for the table.
    ///     schema (Optional[pyarrow.Schema]): Arrow schema for the table.
    ///     description (Optional[str]): Human-readable description for this table.
    ///
    /// Returns:
    ///     TablePaymentOffers: A new TablePaymentOffers object.
    #[new]
    #[pyo3(signature = (table_name, price_tags, schema=None, description=None))]
    fn new(
        table_name: String,
        price_tags: Vec<PyPriceTag>,
        schema: Option<&Bound<'_, PyAny>>,
        description: Option<String>,
    ) -> PyResult<Self> {
        let price_tags: Vec<PriceTag> = price_tags.into_iter().map(|pt| pt.inner).collect();
        let schema_inner = schema
            .map(|s| Schema::from_pyarrow_bound(s))
            .transpose()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let mut offers = TablePaymentOffers::new(table_name, price_tags, schema_inner);
        if let Some(desc) = description {
            offers = offers.with_description(desc);
        }
        Ok(Self { inner: offers })
    }

    /// Create a free table (no payment required).
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///     schema (Optional[pyarrow.Schema]): Arrow schema for the table.
    ///     description (Optional[str]): Human-readable description for this table.
    ///
    /// Returns:
    ///     TablePaymentOffers: A new TablePaymentOffers object.
    #[staticmethod]
    #[pyo3(signature = (table_name, schema=None, description=None))]
    fn new_free_table(
        table_name: String,
        schema: Option<&Bound<'_, PyAny>>,
        description: Option<String>,
    ) -> PyResult<Self> {
        let schema_inner = schema
            .map(|s| Schema::from_pyarrow_bound(s))
            .transpose()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let mut offers = TablePaymentOffers::new_free_table(table_name, schema_inner);
        if let Some(desc) = description {
            offers = offers.with_description(desc);
        }
        Ok(Self { inner: offers })
    }

    /// Set a description for the table payment offers.
    ///
    /// Args:
    ///     description (str): Description of the table.
    ///
    /// Returns:
    ///     TablePaymentOffers: The updated TablePaymentOffers object.
    fn with_description(&mut self, description: String) {
        self.inner = self.inner.clone().with_description(description);
    }

    /// Add a payment offer to the table.
    ///
    /// Args:
    ///     offer (PriceTag): The payment offer to add to the table.
    fn add_payment_offer(&mut self, offer: &PyPriceTag) {
        self.inner = self.inner.clone().add_payment_offer(offer.inner.clone());
    }

    /// Remove a price tag by index.
    ///
    /// Args:
    ///     index (int): Index of the price tag to remove.
    ///
    /// Returns:
    ///     bool: True if the price tag was removed, False if the index was out of bounds.
    fn remove_price_tag(&mut self, index: usize) -> bool {
        self.inner.remove_price_tag(index)
    }

    /// Remove all price tags and mark the table as free (no payment required).
    fn make_free(&mut self) {
        self.inner.make_free();
    }

    /// Get the table name.
    ///
    /// Returns:
    ///     str: The table name.
    #[getter]
    fn table_name(&self) -> &str {
        &self.inner.table_name
    }

    /// Get whether this table requires payment.
    ///
    /// Returns:
    ///     bool: True if the table requires payment.
    #[getter]
    fn requires_payment(&self) -> bool {
        self.inner.requires_payment
    }

    /// Get the table description, or None.
    ///
    /// Returns:
    ///     Optional[str]: The description, or None.
    #[getter]
    fn description(&self) -> Option<&str> {
        self.inner.description.as_deref()
    }

    /// Get the number of price tags.
    ///
    /// Returns:
    ///     int: Number of price tags.
    #[getter]
    fn price_tag_count(&self) -> usize {
        self.inner.price_tags.len()
    }

    /// Get the descriptions of all price tags.
    ///
    /// Returns:
    ///     List[Optional[str]]: List of descriptions, one per price tag (None if a tag has no description).
    #[getter]
    fn price_tag_descriptions(&self) -> Vec<Option<String>> {
        self.inner
            .price_tags
            .iter()
            .map(|pt| pt.description.clone())
            .collect()
    }
}

/// Client for interacting with a payment facilitator.
#[pyclass(name = "FacilitatorClient", from_py_object)]
#[derive(Clone)]
pub struct PyFacilitatorClient {
    inner: FacilitatorClient,
}

#[pymethods]
impl PyFacilitatorClient {
    /// Create a new FacilitatorClient.
    ///
    /// Args:
    ///     base_url (str): Base URL of the facilitator service.
    /// Returns:
    ///     FacilitatorClient: A client for interacting with the facilitator service.
    #[new]
    fn new(base_url: &str) -> PyResult<Self> {
        let url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let client = FacilitatorClient::try_new(url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: client })
    }

    /// Get the base URL of the facilitator.
    ///
    /// Returns:
    ///     str: The base URL.
    #[getter]
    fn base_url(&self) -> String {
        self.inner.base_url().to_string()
    }

    /// Get the verify endpoint URL.
    ///
    /// Returns:
    ///     str: The verify URL.
    #[getter]
    fn verify_url(&self) -> String {
        self.inner.verify_url().to_string()
    }

    /// Get the settle endpoint URL.
    ///
    /// Returns:
    ///     str: The settle URL.
    #[getter]
    fn settle_url(&self) -> String {
        self.inner.settle_url().to_string()
    }

    /// Get the configured timeout in milliseconds, or None.
    ///
    /// Returns:
    ///     Optional[int]: Timeout in milliseconds, or None if not set.
    #[getter]
    fn timeout_ms(&self) -> Option<u64> {
        self.inner.timeout().map(|d| d.as_millis() as u64)
    }

    /// Set custom headers for all future requests.
    ///
    /// Args:
    ///     headers (Dict[str, str]): Headers to set.
    fn set_headers(&mut self, headers: std::collections::HashMap<String, String>) -> PyResult<()> {
        let mut header_map = http::HeaderMap::new();
        for (key, value) in headers {
            let name = http::header::HeaderName::from_str(&key)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            let val = http::header::HeaderValue::from_str(&value)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            header_map.insert(name, val);
        }
        self.inner = self.inner.with_headers(header_map);
        Ok(())
    }

    /// Set a timeout for all future requests.
    ///
    /// Args:
    ///     timeout_ms (int): Timeout in milliseconds.
    fn set_timeout(&mut self, timeout_ms: u64) {
        self.inner = self
            .inner
            .with_timeout(std::time::Duration::from_millis(timeout_ms));
    }
}

/// Global configuration for payment information and facilitator.
#[pyclass(name = "GlobalPaymentConfig")]
pub struct PyGlobalPaymentConfig {
    inner: GlobalPaymentConfig,
}

#[pymethods]
impl PyGlobalPaymentConfig {
    /// Create a new GlobalPaymentConfig.
    ///
    /// Args:
    ///     facilitator (FacilitatorClient): Facilitator client.
    ///     mime_type (Optional[str]): Response MIME type (default: "application/vnd.apache.arrow.stream").
    ///     max_timeout_seconds (Optional[int]): How long a payment offer remains valid in seconds (default: 300).
    ///     default_description (Optional[str]): Fallback description for tables without their own (default: "Query execution payment").
    ///
    /// Returns:
    ///     GlobalPaymentConfig: A new GlobalPaymentConfig object.
    #[new]
    #[pyo3(signature = (facilitator, mime_type=None, max_timeout_seconds=None, default_description=None))]
    fn new(
        facilitator: &PyFacilitatorClient,
        mime_type: Option<String>,
        max_timeout_seconds: Option<u64>,
        default_description: Option<String>,
    ) -> PyResult<Self> {
        let facilitator = std::sync::Arc::new(facilitator.inner.clone());
        Ok(Self {
            inner: GlobalPaymentConfig::new(
                facilitator,
                mime_type,
                max_timeout_seconds,
                default_description,
                None,
            ),
        })
    }

    /// Add a table payment offer to the global config.
    ///
    /// Args:
    ///     offer (TablePaymentOffers): The table payment offer to add to the global config.
    ///
    /// Returns:
    ///     GlobalPaymentConfig: The updated GlobalPaymentConfig object with the new table payment offer.
    fn add_offers_table(&mut self, offer: &PyTablePaymentOffers) {
        self.inner.add_offers_table(offer.inner.clone());
    }

    /// Check if a table requires payment.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///
    /// Returns:
    ///     bool: True if the table requires payment, False otherwise.
    fn table_requires_payment(&self, table_name: &str) -> Option<bool> {
        self.inner.table_requires_payment(table_name)
    }

    /// Set the facilitator client.
    ///
    /// Args:
    ///     facilitator (FacilitatorClient): The new facilitator client.
    fn set_facilitator(&mut self, facilitator: &PyFacilitatorClient) {
        self.inner
            .set_facilitator(Arc::new(facilitator.inner.clone()));
    }

    /// Set the MIME type advertised to clients.
    ///
    /// Args:
    ///     mime_type (str): The MIME type (e.g., "application/vnd.apache.arrow.stream").
    fn set_mime_type(&mut self, mime_type: String) {
        self.inner.set_mime_type(mime_type);
    }

    /// Set how long a payment offer remains valid.
    ///
    /// Args:
    ///     max_timeout_seconds (int): Timeout in seconds.
    fn set_max_timeout_seconds(&mut self, max_timeout_seconds: u64) {
        self.inner.set_max_timeout_seconds(max_timeout_seconds);
    }

    /// Set the fallback description for tables without their own.
    ///
    /// Args:
    ///     default_description (str): The default description.
    fn set_default_description(&mut self, default_description: String) {
        self.inner.set_default_description(default_description);
    }

    /// Get the MIME type advertised to clients.
    ///
    /// Returns:
    ///     str: The MIME type.
    #[getter]
    fn mime_type(&self) -> &str {
        &self.inner.mime_type
    }

    /// Get how long a payment offer remains valid, in seconds.
    ///
    /// Returns:
    ///     int: Timeout in seconds.
    #[getter]
    fn max_timeout_seconds(&self) -> u64 {
        self.inner.max_timeout_seconds
    }

    /// Get the fallback description.
    ///
    /// Returns:
    ///     str: The default description.
    #[getter]
    fn default_description(&self) -> &str {
        &self.inner.default_description
    }
}

// ───── Database wrapper classes ─────

/// DuckDB database backend.
#[cfg(feature = "duckdb")]
#[pyclass(name = "DuckDbDatabase")]
pub struct PyDuckDbDatabase {
    inner: Arc<dyn Database>,
}

#[cfg(feature = "duckdb")]
#[pymethods]
impl PyDuckDbDatabase {
    /// Create a new DuckDbDatabase.
    ///
    /// Args:
    ///     db_path (str): Path to the DuckDB database file.
    ///
    /// Returns:
    ///     DuckDbDatabase: A new DuckDbDatabase object.
    #[new]
    fn new(db_path: &str) -> PyResult<Self> {
        let db = DuckDbDatabase::from_path(db_path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(db),
        })
    }

    /// Get the schema of a table as a pyarrow.Schema.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///
    /// Returns:
    ///     pyarrow.Schema: The Arrow schema of the table.
    fn get_table_schema<'py>(&self, table_name: &str, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let schema = rt
            .block_on(self.inner.get_table_schema(table_name))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let py_schema = schema
            .to_pyarrow(py)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(py_schema.unbind())
    }
}

/// PostgreSQL database backend.
#[cfg(feature = "postgresql")]
#[pyclass(name = "PostgresqlDatabase")]
pub struct PyPostgresqlDatabase {
    inner: Arc<dyn Database>,
}

#[cfg(feature = "postgresql")]
#[pymethods]
impl PyPostgresqlDatabase {
    /// Create a new PostgresqlDatabase.
    ///
    /// Args:
    ///     connection_string (str): PostgreSQL connection string (e.g., "host=localhost port=5432 user=postgres password=pass dbname=mydb").
    ///
    /// Returns:
    ///     PostgresqlDatabase: A new PostgresqlDatabase object.
    #[new]
    fn new(connection_string: &str) -> PyResult<Self> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let db = rt
            .block_on(PostgresqlDatabase::from_connection_string(
                connection_string,
            ))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(db),
        })
    }

    /// Create a new PostgresqlDatabase with full control over connection and pool parameters.
    ///
    /// Args:
    ///     host (str): Database host (e.g., "localhost").
    ///     port (int): Database port (e.g., 5432).
    ///     user (str): Database user.
    ///     password (str): Database password.
    ///     dbname (str): Database name.
    ///     max_pool_size (Optional[int]): Maximum number of connections in the pool (default: 16).
    ///     wait_timeout_ms (Optional[int]): Max time in ms to wait for a connection from the pool.
    ///     create_timeout_ms (Optional[int]): Max time in ms to create a new connection.
    ///     recycle_timeout_ms (Optional[int]): Max time in ms to recycle a connection.
    ///     recycling_method (Optional[str]): Connection recycling strategy: "fast" (default), "verified", or "clean".
    ///
    /// Returns:
    ///     PostgresqlDatabase: A new PostgresqlDatabase object.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    fn from_params(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        dbname: &str,
        max_pool_size: Option<usize>,
        wait_timeout_ms: Option<u64>,
        create_timeout_ms: Option<u64>,
        recycle_timeout_ms: Option<u64>,
        recycling_method: Option<&str>,
    ) -> PyResult<Self> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let db = rt
            .block_on(PostgresqlDatabase::from_params(
                host,
                port,
                user,
                password,
                dbname,
                max_pool_size,
                wait_timeout_ms,
                create_timeout_ms,
                recycle_timeout_ms,
                recycling_method,
            ))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(db),
        })
    }

    /// Get the schema of a table as a pyarrow.Schema.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///
    /// Returns:
    ///     pyarrow.Schema: The Arrow schema of the table.
    fn get_table_schema<'py>(&self, table_name: &str, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let schema = rt
            .block_on(self.inner.get_table_schema(table_name))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let py_schema = schema
            .to_pyarrow(py)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(py_schema.unbind())
    }
}

/// ClickHouse database backend.
#[cfg(feature = "clickhouse")]
#[pyclass(name = "ClickHouseDatabase")]
pub struct PyClickHouseDatabase {
    inner: Arc<dyn Database>,
}

#[cfg(feature = "clickhouse")]
#[pymethods]
impl PyClickHouseDatabase {
    /// Create a new ClickHouseDatabase.
    ///
    /// Args:
    ///     url (str): ClickHouse HTTP endpoint URL (e.g., "http://localhost:8123").
    ///     user (Optional[str]): Database user.
    ///     password (Optional[str]): Database password.
    ///     database (Optional[str]): Database name.
    ///     access_token (Optional[str]): Access token for authentication.
    ///     compression (Optional[str]): Compression mode: "none" or "lz4".
    ///     options (Optional[Dict[str, str]]): Additional ClickHouse settings as key-value pairs.
    ///     headers (Optional[Dict[str, str]]): Additional HTTP headers as key-value pairs.
    ///
    /// Returns:
    ///     ClickHouseDatabase: A new ClickHouseDatabase object.
    #[new]
    #[pyo3(signature = (url, user=None, password=None, database=None, access_token=None, compression=None, options=None, headers=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        url: &str,
        user: Option<&str>,
        password: Option<&str>,
        database: Option<&str>,
        access_token: Option<&str>,
        compression: Option<&str>,
        options: Option<std::collections::HashMap<String, String>>,
        headers: Option<std::collections::HashMap<String, String>>,
    ) -> PyResult<Self> {
        let options_vec = options.map(|m| m.into_iter().collect::<Vec<_>>());
        let headers_vec = headers.map(|m| m.into_iter().collect::<Vec<_>>());
        let db = ClickHouseDatabase::from_params(
            url,
            user,
            password,
            database,
            access_token,
            compression,
            options_vec,
            headers_vec,
        )
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(db),
        })
    }

    /// Get the schema of a table as a pyarrow.Schema.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///
    /// Returns:
    ///     pyarrow.Schema: The Arrow schema of the table.
    fn get_table_schema<'py>(&self, table_name: &str, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let schema = rt
            .block_on(self.inner.get_table_schema(table_name))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let py_schema = schema
            .to_pyarrow(py)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(py_schema.unbind())
    }
}

/// Application state, object mutually shared between API handlers, including database and payment config.
#[pyclass(name = "AppState")]
pub struct PyAppState {
    inner: AppState,
}

#[pymethods]
impl PyAppState {
    /// Create a new AppState.
    ///
    /// Args:
    ///     database: A database object (DuckDbDatabase, PostgresqlDatabase, or ClickHouseDatabase).
    ///     payment_config (GlobalPaymentConfig): Global payment config.
    ///     server_base_url (str): Public URL for the server (e.g., "https://api.tiders.com").
    ///         Used for building resource URLs in payment requirements.
    ///     server_bind_address (str): Address and port to bind the server to (e.g., "0.0.0.0:4021").
    ///
    /// Returns:
    ///     AppState: A new AppState object.
    #[new]
    #[allow(unused_variables)]
    fn new(
        database: &Bound<'_, PyAny>,
        payment_config: &PyGlobalPaymentConfig,
        server_base_url: &str,
        server_bind_address: &str,
    ) -> PyResult<Self> {
        let server_base_url = Url::parse(server_base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let server_bind_address = server_bind_address.to_string();
        // Try to downcast to each database type
        #[cfg(feature = "duckdb")]
        if let Ok(db) = database.extract::<PyRef<PyDuckDbDatabase>>() {
            return Ok(Self {
                inner: AppState {
                    db: db.inner.clone(),
                    payment_config: Arc::new(payment_config.inner.clone()),
                    server_base_url: server_base_url.clone(),
                    server_bind_address: server_bind_address.clone(),
                },
            });
        }
        #[cfg(feature = "postgresql")]
        if let Ok(db) = database.extract::<PyRef<PyPostgresqlDatabase>>() {
            return Ok(Self {
                inner: AppState {
                    db: db.inner.clone(),
                    payment_config: Arc::new(payment_config.inner.clone()),
                    server_base_url: server_base_url.clone(),
                    server_bind_address: server_bind_address.clone(),
                },
            });
        }
        #[cfg(feature = "clickhouse")]
        if let Ok(db) = database.extract::<PyRef<PyClickHouseDatabase>>() {
            return Ok(Self {
                inner: AppState {
                    db: db.inner.clone(),
                    payment_config: Arc::new(payment_config.inner.clone()),
                    server_base_url: server_base_url.clone(),
                    server_bind_address: server_bind_address.clone(),
                },
            });
        }

        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "Expected a DuckDbDatabase, PostgresqlDatabase, or ClickHouseDatabase object",
        ))
    }
}

/// Start a payment-enabled server (blocking call).
///
/// Args:
///     state (AppState): Application state with database and payment config.
#[pyfunction]
fn start_server_py(state: &PyAppState) -> PyResult<()> {
    let state = Arc::new(state.inner.clone());
    let rt = Runtime::new()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    rt.block_on(async {
        start_server(state).await;
        Ok(())
    })
}
