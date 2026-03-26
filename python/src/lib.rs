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
use url::Url;
use tokio::runtime::Runtime;
use arrow::datatypes::Schema;

use tiders_x402::{PriceTag, TablePaymentOffers, GlobalPaymentConfig, AppState, FacilitatorClient};
#[cfg(any(feature = "duckdb", feature = "postgresql", feature = "clickhouse"))]
use tiders_x402::Database;
use tiders_x402::price::TokenAmount;
#[cfg(feature = "duckdb")]
use tiders_x402::database_duckdb::DuckDbDatabase;
#[cfg(feature = "postgresql")]
use tiders_x402::database_postgresql::PostgresqlDatabase;
#[cfg(feature = "clickhouse")]
use tiders_x402::database_clickhouse::ClickHouseDatabase;
use x402_chain_eip155::chain::{ChecksummedAddress, Eip155TokenDeployment};
use x402_chain_eip155::KnownNetworkEip155;
use x402_types::networks::USDC;
use alloy::primitives::U256;
use arrow::pyarrow::FromPyArrow;
#[cfg(any(feature = "duckdb", feature = "postgresql", feature = "clickhouse"))]
use arrow::pyarrow::ToPyArrow;

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
#[pyclass(name="PriceTag", from_py_object)]
#[derive(Clone)]
pub struct PyPriceTag {
    inner: PriceTag,
}

#[pymethods]
impl PyPriceTag {
    /// Create a new PriceTag.
    ///
    /// Args:
    ///     pay_to (str): EVM address to pay to.
    ///     amount_per_item (Union[str, int]): Amount per item (rows or tuples). If a string (e.g., "0.002" or "$1.23") it is interpreted as a MoneyAmount and converted to a TokenAmount using decimals from the token. If an integer it is interpreted as an amount in the smallest token unit ( without decimals, e.g. 1000000 for 1 USDC).
    ///     token (USDC): Token with decimals and EIP712 information, currently only USDC is supported.
    ///     min_total_amount (Optional[Union[str, int]]): Minimum total amount for this offer to be valid (optional). Can be a string (e.g., "0.01") or an integer representing the smallest token unit.
    ///     min_items (Optional[int]): Minimum number of items (rows or tuples) for this offer to be valid (optional).
    ///     max_items (Optional[int]): Maximum number of items (rows or tuples) for this offer to be valid (optional).
    ///     description (Optional[str]): Description of the offer (optional).
    ///     is_default (bool): Whether this is the default offer.
    ///
    /// Returns:
    ///     PriceTag: A new PriceTag object.
    #[new]
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

        // Handle amount_per_item as either string or integer
        let amount_per_item = if let Ok(amount_str) = amount_per_item.extract::<String>(py) {
            // Parse as string (MoneyAmount) using token's parse method
            let deployed_amount = token_deployment.parse(amount_str.as_str())
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            TokenAmount(deployed_amount.amount)
        } else if let Ok(amount_int) = amount_per_item.extract::<i64>(py) {
            // Parse as integer - treat as smallest token unit
            if amount_int < 0 {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Amount cannot be negative"));
            }
            TokenAmount(U256::from(amount_int as u64))
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "amount_per_item must be either a string (e.g., '0.001') or an integer representing the smallest token unit"
            ));
        };

        let min_total_amount = if let Some(min_obj) = min_total_amount {
            if let Ok(min_str) = min_obj.extract::<String>(py) {
                let deployed_amount = token_deployment.parse(min_str.as_str())
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
                Some(TokenAmount(deployed_amount.amount))
            } else if let Ok(min_int) = min_obj.extract::<i64>(py) {
                if min_int < 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Min total amount cannot be negative"));
                }
                Some(TokenAmount(U256::from(min_int as u64)))
            } else {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "min_total_amount must be either a string (e.g., '0.01') or an integer representing the smallest token unit"
                ));
            }
        } else {
            None
        };

        Ok(Self {
            inner: PriceTag {
                pay_to,
                amount_per_item,
                token: token_deployment.clone(),
                min_total_amount,
                min_items,
                max_items,
                description,
                is_default,
            },
        })
    }
}

/// Represents a USDC token on a supported network.
#[pyclass(name="USDC", from_py_object)]
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
            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Invalid network: {}. Supported: base_sepolia, base, avalanche_fuji, avalanche, polygon, polygon_amoy", network_str)
            )),
        };

        Ok(Self { inner: deployment })
    }
}

/// Holds payment offers for a table.
#[pyclass(name="TablePaymentOffers", from_py_object)]
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
    ///
    /// Returns:
    ///     TablePaymentOffers: A new TablePaymentOffers object.
    #[new]
    fn new(table_name: String, price_tags: Vec<PyPriceTag>, schema: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let price_tags: Vec<PriceTag> = price_tags.into_iter().map(|pt| pt.inner).collect();
        let schema_inner = schema
            .map(|s| Schema::from_pyarrow_bound(s))
            .transpose()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self {
            inner: TablePaymentOffers::new(table_name, price_tags, schema_inner),
        })
    }

    /// Create a free table (no payment required).
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///     schema (Optional[pyarrow.Schema]): Arrow schema for the table.
    ///
    /// Returns:
    ///     TablePaymentOffers: A new TablePaymentOffers object.
    #[staticmethod]
    fn new_free_table(table_name: String, schema: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let schema_inner = schema
            .map(|s| Schema::from_pyarrow_bound(s))
            .transpose()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self {
            inner: TablePaymentOffers::new_free_table(table_name, schema_inner),
        })
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
    ///
    /// Returns:
    ///     TablePaymentOffers: The updated TablePaymentOffers object with the new payment offer.
    fn with_payment_offer(&mut self, offer: &PyPriceTag) {
        self.inner = self.inner.clone().with_payment_offer(offer.inner.clone());
    }
}

/// Client for interacting with a payment facilitator.
#[pyclass(name="FacilitatorClient", from_py_object)]
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
}

/// Global configuration for payment information and facilitator.
#[pyclass(name="GlobalPaymentConfig")]
pub struct PyGlobalPaymentConfig {
    inner: GlobalPaymentConfig,
}

#[pymethods]
impl PyGlobalPaymentConfig {
    /// Create a new GlobalPaymentConfig.
    ///
    /// Args:
    ///     facilitator (FacilitatorClient): Facilitator client.
    ///     base_url (str): Base URL for the app.
    ///
    /// Returns:
    ///     GlobalPaymentConfig: A new GlobalPaymentConfig object.
    #[new]
    fn new(facilitator: &PyFacilitatorClient, base_url: &str) -> PyResult<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let facilitator = std::sync::Arc::new(facilitator.inner.clone());
        Ok(Self {
            inner: GlobalPaymentConfig::default(facilitator, base_url),
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
}

// ───── Database wrapper classes ─────

/// DuckDB database backend.
#[cfg(feature = "duckdb")]
#[pyclass(name="DuckDbDatabase")]
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
        Ok(Self { inner: Arc::new(db) })
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
        let schema = rt.block_on(self.inner.get_table_schema(table_name))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let py_schema = schema.to_pyarrow(py)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(py_schema.unbind())
    }
}

/// PostgreSQL database backend.
#[cfg(feature = "postgresql")]
#[pyclass(name="PostgresqlDatabase")]
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
        let db = rt.block_on(PostgresqlDatabase::from_connection_string(connection_string))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: Arc::new(db) })
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
        let db = rt.block_on(PostgresqlDatabase::from_params(
            host, port, user, password, dbname,
            max_pool_size, wait_timeout_ms, create_timeout_ms, recycle_timeout_ms,
            recycling_method,
        ))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: Arc::new(db) })
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
        let schema = rt.block_on(self.inner.get_table_schema(table_name))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let py_schema = schema.to_pyarrow(py)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(py_schema.unbind())
    }
}

/// ClickHouse database backend.
#[cfg(feature = "clickhouse")]
#[pyclass(name="ClickHouseDatabase")]
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
    ///
    /// Returns:
    ///     ClickHouseDatabase: A new ClickHouseDatabase object.
    #[new]
    fn new(url: &str) -> PyResult<Self> {
        let db = ClickHouseDatabase::from_url(url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: Arc::new(db) })
    }

    /// Create a new ClickHouseDatabase with full client configuration.
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
    #[staticmethod]
    fn from_params(
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
            url, user, password, database, access_token, compression,
            options_vec, headers_vec,
        )
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: Arc::new(db) })
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
        let schema = rt.block_on(self.inner.get_table_schema(table_name))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let py_schema = schema.to_pyarrow(py)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(py_schema.unbind())
    }
}

/// Application state, object mutually shared between API handlers, including database and payment config.
#[pyclass(name="AppState")]
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
    ///
    /// Returns:
    ///     AppState: A new AppState object.
    #[new]
    #[allow(unused_variables)]
    fn new(database: &Bound<'_, PyAny>, payment_config: &PyGlobalPaymentConfig) -> PyResult<Self> {
        // Try to downcast to each database type
        #[cfg(feature = "duckdb")]
        if let Ok(db) = database.extract::<PyRef<PyDuckDbDatabase>>() {
            return Ok(Self {
                inner: AppState {
                    db: db.inner.clone(),
                    payment_config: Arc::new(payment_config.inner.clone()),
                },
            });
        }
        #[cfg(feature = "postgresql")]
        if let Ok(db) = database.extract::<PyRef<PyPostgresqlDatabase>>() {
            return Ok(Self {
                inner: AppState {
                    db: db.inner.clone(),
                    payment_config: Arc::new(payment_config.inner.clone()),
                },
            });
        }
        #[cfg(feature = "clickhouse")]
        if let Ok(db) = database.extract::<PyRef<PyClickHouseDatabase>>() {
            return Ok(Self {
                inner: AppState {
                    db: db.inner.clone(),
                    payment_config: Arc::new(payment_config.inner.clone()),
                },
            });
        }

        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "Expected a DuckDbDatabase, PostgresqlDatabase, or ClickHouseDatabase object"
        ))
    }
}

/// Start a payment-enabled server (blocking call).
///
/// Args:
///     state (AppState): Application state with database and payment config.
///     base_url (str): Base URL for the server (e.g., "http://0.0.0.0:4021").
#[pyfunction]
fn start_server_py(state: &PyAppState, base_url: &str) -> PyResult<()> {
    let base_url = Url::parse(base_url)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
    let state = Arc::new(state.inner.clone());
    let rt = Runtime::new()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    rt.block_on(async {
        tiders_x402::start_server(state, base_url).await;
        Ok(())
    })
}
