//! A whole database in one object: disk, buffer pool and catalog, and `execute_sql`, which takes SQL text and writes the result.
//!
//! A query goes through the **parser** (`crate::sql`), which makes a syntax tree; the **binder**, which resolves its names; the **planner**,
//! which makes a plan; the **optimizer**, which rewrites it; and the **execution engine**, which runs it.
//!
//! The buffer pool is leaked (`Box::leak`) so that the catalog, which borrows it, can live in the same struct. Every instance keeps its
//! pool for the life of the program, so reuse instances.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use crate::binder::binder::Binder;
use crate::binder::bound_statement::{explain_options, BoundStatement};
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::catalog::catalog::Catalog;
use crate::catalog::schema::Schema;
use crate::catalog::table_generator::generate_test_tables;
use crate::concurrency::transaction::Transaction;
use crate::concurrency::transaction_manager::TransactionManager;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::result_writer::ResultWriter;
use crate::execution::analyze_stats::AnalyzeStats;
use crate::execution::check_options::CheckOptions;
use crate::execution::execution_engine::ExecutionEngine;
use crate::execution::executor_context::ExecutorContext;
use crate::execution::executors::mock_scan_executor::{get_mock_table_schema_of, MOCK_TABLE_LIST};
use crate::optimizer::optimizer::Optimizer;
use crate::planner::planner::Planner;
use crate::sql::parser;
use crate::storage::disk::disk_manager::DiskManager;
use crate::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;

pub struct BusTubInstance {
    pub buffer_pool_manager: &'static BufferPoolManager,
    /// Leaked like the buffer pool: the transaction manager needs the catalog for as long as the instance lives.
    pub catalog: &'static RwLock<Catalog<'static>>,
    pub txn_manager: TransactionManager,
    session_variables: Mutex<HashMap<String, String>>,
}

impl BusTubInstance {
    /// A database in memory (nothing is written to disk), with a buffer pool of `bpm_size` frames.
    pub fn new(bpm_size: usize) -> BusTubInstance {
        Self::from_disk(bpm_size, Arc::new(DiskManagerUnlimitedMemory::new()))
    }

    /// A database in the file `db_file_name`.
    pub fn with_file(db_file_name: impl AsRef<Path>, bpm_size: usize) -> std::io::Result<BusTubInstance> {
        Ok(Self::from_disk(bpm_size, Arc::new(DiskManager::new(db_file_name)?)))
    }

    fn from_disk(bpm_size: usize, disk: Arc<dyn crate::storage::disk::disk_manager::DiskIo>) -> BusTubInstance {
        let bpm: &'static BufferPoolManager = Box::leak(Box::new(BufferPoolManager::new(bpm_size, disk)));
        let catalog: &'static RwLock<Catalog<'static>> = Box::leak(Box::new(RwLock::new(Catalog::new(bpm))));
        BusTubInstance { buffer_pool_manager: bpm, catalog, txn_manager: TransactionManager::new(catalog), session_variables: Mutex::new(HashMap::new()) }
    }

    /// Creates the `__mock_*` tables (their rows are made up on the fly by the mock scan).
    pub fn generate_mock_table(&self) {
        let mut catalog = self.catalog.write().unwrap();
        for name in MOCK_TABLE_LIST {
            let schema = get_mock_table_schema_of(name).expect("every listed mock table has a schema");
            catalog.create_table(name, &schema);
        }
    }

    /// Creates the `test_*` tables (real tables, filled at once).
    pub fn generate_test_table(&self) {
        generate_test_tables(&mut self.catalog.write().unwrap()).expect("filling the test tables");
    }

    pub fn get_session_variable(&self, key: &str) -> String {
        self.session_variables.lock().unwrap().get(key).cloned().unwrap_or_default()
    }

    fn is_force_starter_rule(&self) -> bool {
        matches!(self.get_session_variable("force_optimizer_starter_rule").to_lowercase().as_str(), "1" | "true" | "yes")
    }

    /// Runs the SQL (possibly several statements, separated by `;`) and writes each result to `writer`. Returns whether every statement
    /// executed successfully (a statement that fails with an `Execution` error returns `false`; any other error is an `Err`).
    pub fn execute_sql(&self, sql: &str, writer: &mut dyn ResultWriter, check_options: Option<&CheckOptions>) -> Result<bool> {
        self.execute_sql_in(sql, writer, check_options, None)
    }

    /// Runs the SQL inside `txn`: reads see the versions `txn` can see, and writes leave versions behind for the others.
    pub fn execute_sql_txn(&self, sql: &str, writer: &mut dyn ResultWriter, txn: &Arc<Transaction>) -> Result<bool> {
        self.execute_sql_in(sql, writer, None, Some(txn))
    }

    fn execute_sql_in(&self, sql: &str, writer: &mut dyn ResultWriter, check_options: Option<&CheckOptions>, txn: Option<&Arc<Transaction>>) -> Result<bool> {
        if let Some(txn) = txn {
            if txn.state() != crate::concurrency::transaction::TransactionState::Running {
                return Err(Exception::new(ExceptionType::Execution, "the transaction is not running"));
            }
        }
        if sql.starts_with('\\') {
            return self.execute_command(sql, writer);
        }
        let statements = parser::parse(sql)?;
        self.execute_statements(&statements, writer, check_options, txn)
    }

    /// Runs already parsed statements (what `execute_sql` does after the parser; prepared statements start here).
    pub fn execute_statements(
        &self,
        statements: &[crate::sql::ast::Statement],
        writer: &mut dyn ResultWriter,
        check_options: Option<&CheckOptions>,
        txn: Option<&Arc<Transaction>>,
    ) -> Result<bool> {
        let mut is_successful = true;
        for stmt in statements {
            let bound = {
                let catalog = self.catalog.read().unwrap();
                Binder::new(&catalog).bind_statement(stmt)?
            };
            match &bound {
                BoundStatement::Create { table, columns, primary_key } => {
                    self.handle_create_statement(table, columns, primary_key, writer)?;
                    continue;
                }
                BoundStatement::Index { index_name, table, cols, index_type } => {
                    self.handle_index_statement(index_name, table, cols, index_type, writer)?;
                    continue;
                }
                BoundStatement::VariableShow { variable } => {
                    writer.one_cell(&format!("{variable}={}", self.get_session_variable(variable)));
                    continue;
                }
                BoundStatement::VariableSet { variable, value } => {
                    self.session_variables.lock().unwrap().insert(variable.clone(), value.clone());
                    continue;
                }
                BoundStatement::Explain { statement, options } => {
                    self.handle_explain_statement(statement, *options, writer)?;
                    continue;
                }
                BoundStatement::Transaction(kind) => {
                    writer.one_cell(&format!("{kind} is only supported in managed txn mode, please use bustub-shell"));
                    continue;
                }
                _ => {}
            }
            let is_modify = matches!(bound, BoundStatement::Delete { .. } | BoundStatement::Update { .. });
            let catalog = self.catalog.read().unwrap();
            let mut planner = Planner::new(&catalog);
            planner.plan_query(&bound)?;
            let plan = planner.plan.clone().expect("plan_query sets the plan");
            let optimized = Optimizer::new(&catalog, self.is_force_starter_rule()).optimize(&plan)?;
            let mut ctx = ExecutorContext::new(&catalog, self.buffer_pool_manager, is_modify);
            if let Some(options) = check_options {
                ctx = ctx.with_check_options(options.clone());
            }
            if let Some(txn) = txn {
                ctx = ctx.with_txn(txn.clone(), &self.txn_manager);
            }
            let (ok, result_set) = ExecutionEngine::execute(&optimized, &ctx)?;
            is_successful &= ok;
            let schema = plan.output_schema();
            writer.begin_table(false);
            writer.begin_header();
            for column in schema.columns() {
                writer.write_header_cell(column.name());
            }
            writer.end_header();
            for tuple in &result_set {
                writer.begin_row();
                for i in 0..schema.column_count() {
                    writer.write_cell(&tuple.get_value(schema, i).to_string());
                }
                writer.end_row();
            }
            writer.end_table();
        }
        Ok(is_successful)
    }

    fn execute_command(&self, command: &str, writer: &mut dyn ResultWriter) -> Result<bool> {
        let catalog = self.catalog.read().unwrap();
        match command {
            "\\dt" => {
                writer.begin_table(false);
                writer.begin_header();
                for h in ["oid", "name", "cols"] {
                    writer.write_header_cell(h);
                }
                writer.end_header();
                let mut infos: Vec<_> = catalog.get_table_names().iter().map(|n| catalog.get_table(n).unwrap()).collect();
                infos.sort_by_key(|i| i.oid);
                for info in infos {
                    writer.begin_row();
                    writer.write_cell(&info.oid.to_string());
                    writer.write_cell(&info.name);
                    writer.write_cell(&info.schema.to_string(true));
                    writer.end_row();
                }
                writer.end_table();
            }
            "\\di" => {
                writer.begin_table(false);
                writer.begin_header();
                for h in ["table_name", "index_oid", "index_name", "index_cols"] {
                    writer.write_header_cell(h);
                }
                writer.end_header();
                let mut table_names = catalog.get_table_names();
                table_names.sort();
                for table_name in table_names {
                    for index in catalog.get_table_indexes(&table_name) {
                        writer.begin_row();
                        writer.write_cell(&table_name);
                        writer.write_cell(&index.index_oid.to_string());
                        writer.write_cell(&index.name);
                        writer.write_cell(&index.key_schema.to_string(true));
                        writer.end_row();
                    }
                }
                writer.end_table();
            }
            "\\help" => writer.one_cell(HELP),
            other => return Err(Exception::new(ExceptionType::Invalid, format!("unsupported internal command: {other}"))),
        }
        Ok(true)
    }

    // ---- DDL ----------------------------------------------------------------------------------------------------------------

    fn handle_create_statement(&self, table: &str, columns: &[crate::catalog::column::Column], primary_key: &[String], writer: &mut dyn ResultWriter) -> Result<()> {
        let mut catalog = self.catalog.write().unwrap();
        let schema = Schema::new(columns.to_vec());
        let info = catalog.create_table(table, &schema).ok_or_else(|| Exception::new(ExceptionType::Invalid, "Failed to create table"))?;
        let mut index = None;
        if !primary_key.is_empty() {
            let mut col_ids = vec![];
            for col in primary_key {
                let idx = info.schema.try_col_idx(col).ok_or_else(|| Exception::new(ExceptionType::Invalid, format!("Column does not exist: {col}")))?;
                col_ids.push(idx);
            }
            index = catalog.create_index(&format!("{table}_pk"), table, col_ids, true)?;
        }
        match index {
            Some(index) => writer.one_cell(&format!("Table created with id = {}, Primary key index created with id = {}", info.oid, index.index_oid)),
            None => writer.one_cell(&format!("Table created with id = {}", info.oid)),
        }
        Ok(())
    }

    fn handle_index_statement(
        &self,
        index_name: &str,
        table: &crate::binder::bound_table_ref::BoundTableRef,
        cols: &[Vec<String>],
        index_type: &str,
        writer: &mut dyn ResultWriter,
    ) -> Result<()> {
        use crate::binder::bound_table_ref::BoundTableRef;
        let BoundTableRef::Base { table: table_name, schema, .. } = table else { unreachable!() };
        let mut col_ids = vec![];
        for col in cols {
            let name = col.last().expect("a column reference has a name");
            col_ids.push(schema.try_col_idx(name).ok_or_else(|| Exception::new(ExceptionType::Invalid, format!("Column does not exist: {name}")))?);
        }
        if !(index_type.is_empty() || index_type == "bplustree") {
            return Err(Exception::new(ExceptionType::NotImplemented, format!("unsupported index type {index_type}")));
        }
        let mut catalog = self.catalog.write().unwrap();
        let info = catalog
            .create_index(index_name, table_name, col_ids, false)?
            .ok_or_else(|| Exception::new(ExceptionType::Invalid, "Failed to create index"))?;
        writer.one_cell(&format!("Index created with id = {} with type = BPlusTree", info.index_oid));
        Ok(())
    }

    fn handle_explain_statement(&self, statement: &BoundStatement, options: u8, writer: &mut dyn ResultWriter) -> Result<()> {
        let mut output = String::new();
        if options & explain_options::BINDER != 0 {
            output.push_str("=== BINDER ===\n");
            output.push_str(&statement.to_string());
            output.push('\n');
        }
        let catalog = self.catalog.read().unwrap();
        let mut planner = Planner::new(&catalog);
        planner.plan_query(statement)?;
        let plan = planner.plan.clone().expect("plan_query sets the plan");
        let show_schema = options & explain_options::SCHEMA != 0;
        let analyze = options & explain_options::ANALYZE != 0;
        if options & explain_options::PLANNER != 0 {
            output.push_str("=== PLANNER ===\n");
            output.push_str(&plan.to_string_with(show_schema));
            output.push('\n');
        }
        let optimized = Optimizer::new(&catalog, self.is_force_starter_rule()).optimize(&plan)?;
        if options & explain_options::OPTIMIZER != 0 {
            output.push_str("=== OPTIMIZER ===\n");
            output.push_str(&optimized.to_string_with(show_schema));
            output.push('\n');
        }
        if analyze {
            // @begin 3i-06
            use crate::execution::analyze_render::render_analysis;
            if !matches!(statement, BoundStatement::Select(_)) {
                return Err(Exception::new(ExceptionType::NotImplemented, "EXPLAIN ANALYZE of a statement that changes data is not supported"));
            }
            let stats = Arc::new(AnalyzeStats::default());
            let ctx = ExecutorContext::new(&catalog, self.buffer_pool_manager, false).with_analyze(stats.clone());
            ExecutionEngine::execute(&optimized, &ctx)?;
            output.push_str("=== ANALYZE ===\n");
            output.push_str(&render_analysis(&optimized, &stats));
            //~ return Err(Exception::new(ExceptionType::NotImplemented, "EXPLAIN ANALYZE is not supported yet"));
            // @end
        }
        writer.one_cell(&output);
        Ok(())
    }
}

const HELP: &str = "Welcome to the BusTub shell!
\\dt: show all tables
\\di: show all indices
\\help: show this message again
BusTub shell currently only supports a small set of Postgres queries. Use `explain` to see the execution plan of your query.
";
