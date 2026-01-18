//! DAG (Directed Acyclic Graph) Executor.
//!
//! Handles the parallel execution of tasks with dependencies.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use multi_agent_core::{Error, Result};

/// A unit of work in the DAG.
#[async_trait::async_trait]
pub trait DagTask: Send + Sync {
    /// Get the unique name of the task.
    fn name(&self) -> &str;
    /// Get the names of tasks this task depends on.
    fn dependencies(&self) -> &[String];
    /// Execute the task given the results of previous tasks.
    async fn execute(&self, context: &HashMap<String, String>) -> Result<String>;
}

/// Executor for a DAG of tasks.
pub struct DagExecutor {
    allow_parallel: bool,
}

impl DagExecutor {
    /// Create a new DAG executor.
    pub fn new(allow_parallel: bool) -> Self {
        Self { allow_parallel }
    }

    /// Execute the DAG tasks.
    pub async fn execute<T>(&self, tasks: Vec<T>) -> Result<HashMap<String, String>>
    where
        T: DagTask + 'static,
    {
        if self.allow_parallel {
            self.execute_parallel(tasks).await
        } else {
            self.execute_sequential(tasks).await
        }
    }

    async fn execute_sequential<T>(&self, tasks: Vec<T>) -> Result<HashMap<String, String>>
    where
        T: DagTask,
    {
        let sorted_tasks = self.topological_sort(tasks)?;
        let mut results = HashMap::new();

        for task in sorted_tasks {
            let output = task.execute(&results).await?;
            results.insert(task.name().to_string(), output);
        }

        Ok(results)
    }

    async fn execute_parallel<T>(&self, tasks: Vec<T>) -> Result<HashMap<String, String>>
    where
        T: DagTask + 'static,
    {
        // 1. Build dependency graph
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new(); // dependent -> [dependencies]
        let mut rev_adj_list: HashMap<String, Vec<String>> = HashMap::new(); // dependency -> [dependents]
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        
        // Move tasks into Arcs
        let task_map: HashMap<String, Arc<T>> = tasks.into_iter()
            .map(|t| (t.name().to_string(), Arc::new(t)))
            .collect();

        for (name, task) in &task_map {
            adj_list.entry(name.clone()).or_default();
            rev_adj_list.entry(name.clone()).or_default();
            in_degree.entry(name.clone()).or_insert(0);

            for dep in task.dependencies() {
                if !task_map.contains_key(dep) {
                    return Err(Error::SopExecution(format!("Unknown dependency: {}", dep)));
                }
                
                // dep -> name (dep is a dependency of name)
                rev_adj_list.entry(dep.clone()).or_default().push(name.clone());
                adj_list.entry(name.clone()).or_default().push(dep.clone());
                *in_degree.entry(name.clone()).or_insert(0) += 1;
            }
        }

        // Check for cycles
        if self.detect_cycle(&task_map) {
             return Err(Error::SopExecution("Cycle detected in DAG".to_string()));
        }

        // 2. Execution Loop
        let results = Arc::new(Mutex::new(HashMap::new()));
        let in_degree = Arc::new(Mutex::new(in_degree));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(String, String)>>(100);
        
        // Initial set of tasks (in-degree 0)
        let mut running_count = 0;
        let mut pending_count = task_map.len();

        {
            let in_degree_guard = in_degree.lock().await;
            for (name, &deg) in in_degree_guard.iter() {
                if deg == 0 {
                    let task = task_map[name].clone();
                    let tx = tx.clone();
                    let results = results.clone();
                    
                    running_count += 1;
                    tokio::spawn(async move {
                        // Get dependeny results
                        let context = {
                            results.lock().await.clone()
                        };
                        
                        match task.execute(&context).await {
                            Ok(output) => {
                                let _ = tx.send(Ok((task.name().to_string(), output))).await;
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                            }
                        }
                    });
                }
            }
        }

        while pending_count > 0 {
            if running_count == 0 {
                // This shouldn't happen if graph is valid and no cycles (checked), 
                // unless tasks panic or logic error.
                 return Err(Error::SopExecution("Deadlock detected in DAG execution".to_string())); 
            }

            match rx.recv().await {
                Some(Ok((name, output))) => {
                    running_count -= 1;
                    pending_count -= 1;

                    // Store result
                    results.lock().await.insert(name.clone(), output);

                    // Unlock dependents
                    let dependents = rev_adj_list.get(&name).cloned().unwrap_or_default();
                    let mut in_degree_guard = in_degree.lock().await;

                    for dep_name in dependents {
                        if let Some(deg) = in_degree_guard.get_mut(&dep_name) {
                            *deg -= 1;
                            if *deg == 0 {
                                // Launch task
                                let task = task_map[&dep_name].clone();
                                let tx = tx.clone();
                                let results = results.clone();
                                running_count += 1;
                                
                                tokio::spawn(async move {
                                    let context = results.lock().await.clone();
                                    match task.execute(&context).await {
                                        Ok(output) => {
                                            let _ = tx.send(Ok((task.name().to_string(), output))).await;
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(e)).await;
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
                Some(Err(e)) => return Err(e),
                None => return Err(Error::SopExecution("Channel closed unexpectedly".to_string())),
            }
        }

        let final_results = Arc::try_unwrap(results)
            .map_err(|_| Error::Internal("Failed to unwrap results lock".to_string()))?
            .into_inner();
            
        Ok(final_results)
    }

    fn topological_sort<T>(&self, tasks: Vec<T>) -> Result<Vec<T>>
    where
        T: DagTask,
    {
        // Simple Kahn's algorithm or DFS
        // Since we need to return T objects, we need to move them.
        
        let mut task_map: HashMap<String, T> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        
        for task in tasks {
            let name = task.name().to_string();
            in_degree.insert(name.clone(), 0);
            adj_list.insert(name.clone(), Vec::new());
            task_map.insert(name, task);
        }

        // Build graph
        for (name, task) in &task_map {
            for dep in task.dependencies() {
                if !task_map.contains_key(dep) {
                    return Err(Error::SopExecution(format!("Unknown dependency: {}", dep)));
                }
                adj_list.entry(dep.clone()).or_default().push(name.clone());
                *in_degree.entry(name.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();
            
        // For deterministic sequential execution, sort the queue?
        // Typically queue is a FIFO or standard vector.
        
        let mut sorted = Vec::new();
        while let Some(u) = queue.pop() {
            // Move task to sorted
             if let Some(t) = task_map.remove(&u) {
                 sorted.push(t);
             }

             if let Some(neighbors) = adj_list.get(&u) {
                 for v in neighbors {
                     if let Some(d) = in_degree.get_mut(v) {
                         *d -= 1;
                         if *d == 0 {
                             queue.push(v.clone());
                         }
                     }
                 }
             }
        }

        if !task_map.is_empty() {
             return Err(Error::SopExecution("Cycle detected in DAG".to_string()));
        }

        Ok(sorted)
    }

    fn detect_cycle<T>(&self, _tasks: &HashMap<String, Arc<T>>) -> bool {
        // Topological sort checks this implicitly, but parallel build creates graph first.
        // Parallel executor does implicit cycle detection if pending_count > 0 but running_count == 0.
        // So explicit check not strictly required if we trust that logic, but safe to add.
        // For now relying on deadlock check.
        false
    }
}
