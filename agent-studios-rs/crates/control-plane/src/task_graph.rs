use std::collections::{HashMap, HashSet, VecDeque};

use agent_studios_protocol::id::TaskId;
use agent_studios_protocol::task::{TaskRecord, TaskState};

use crate::error::TaskGraphError;

/// Directed Acyclic Graph (DAG) managing Task execution dependencies.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskGraph {
    tasks: HashMap<TaskId, TaskRecord>,
    /// task_id -> set of prerequisite task_ids that must complete before this task runs
    dependencies: HashMap<TaskId, HashSet<TaskId>>,
    /// task_id -> set of dependent task_ids waiting on this task
    dependents: HashMap<TaskId, HashSet<TaskId>>,
}

impl TaskGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new task to the graph.
    pub fn add_task(&mut self, task: TaskRecord) -> Result<(), TaskGraphError> {
        let task_id = task.id;

        // Verify that declared dependencies exist if any
        for &dep_id in &task.dependencies {
            if dep_id == task_id {
                return Err(TaskGraphError::SelfDependency(task_id));
            }
            if !self.tasks.contains_key(&dep_id) {
                return Err(TaskGraphError::UnknownTask(dep_id));
            }
        }

        // Check for potential cycles introduced by initial dependencies
        for &dep_id in &task.dependencies {
            if self.reaches_iterative(task_id, dep_id) {
                return Err(TaskGraphError::DependencyCycle {
                    from: task_id,
                    to: dep_id,
                });
            }
        }

        // Register dependencies in graph indices
        let dep_set: HashSet<TaskId> = task.dependencies.iter().copied().collect();
        for &dep_id in &dep_set {
            self.dependents.entry(dep_id).or_default().insert(task_id);
        }

        self.dependencies.insert(task_id, dep_set);
        self.dependents.entry(task_id).or_default();
        self.tasks.insert(task_id, task);

        Ok(())
    }

    /// Safely removes a task from the graph.
    /// Fails with `UnsafeTaskRemoval` if other tasks still depend on it.
    pub fn remove_task(&mut self, task_id: TaskId) -> Result<TaskRecord, TaskGraphError> {
        let dependents = self.dependents.get(&task_id).cloned().unwrap_or_default();
        if !dependents.is_empty() {
            return Err(TaskGraphError::UnsafeTaskRemoval {
                task_id,
                dependent_count: dependents.len(),
            });
        }

        let task = self
            .tasks
            .remove(&task_id)
            .ok_or(TaskGraphError::UnknownTask(task_id))?;

        // Clean up outgoing dependency edges
        if let Some(deps) = self.dependencies.remove(&task_id) {
            for dep_id in deps {
                if let Some(set) = self.dependents.get_mut(&dep_id) {
                    set.remove(&task_id);
                }
            }
        }

        self.dependents.remove(&task_id);
        Ok(task)
    }

    /// Adds a dependency edge: `task_id` depends on `dependency_id`.
    /// `dependency_id` must execute before `task_id`.
    pub fn add_dependency(
        &mut self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<(), TaskGraphError> {
        if task_id == dependency_id {
            return Err(TaskGraphError::SelfDependency(task_id));
        }

        if !self.tasks.contains_key(&task_id) {
            return Err(TaskGraphError::UnknownTask(task_id));
        }
        if !self.tasks.contains_key(&dependency_id) {
            return Err(TaskGraphError::UnknownTask(dependency_id));
        }

        // If dependency_id already reaches task_id via existing dependencies,
        // making task_id depend on dependency_id would close a directed cycle.
        if self.reaches_iterative(dependency_id, task_id) {
            return Err(TaskGraphError::DependencyCycle {
                from: task_id,
                to: dependency_id,
            });
        }

        self.dependencies
            .entry(task_id)
            .or_default()
            .insert(dependency_id);
        self.dependents
            .entry(dependency_id)
            .or_default()
            .insert(task_id);

        // Keep TaskRecord.dependencies in sync
        if let Some(task) = self.tasks.get_mut(&task_id)
            && !task.dependencies.contains(&dependency_id)
        {
            task.dependencies.push(dependency_id);
        }

        Ok(())
    }

    /// Removes a dependency edge. Returns true if the dependency was present.
    pub fn remove_dependency(
        &mut self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<bool, TaskGraphError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(TaskGraphError::UnknownTask(task_id));
        }
        if !self.tasks.contains_key(&dependency_id) {
            return Err(TaskGraphError::UnknownTask(dependency_id));
        }

        let removed = self
            .dependencies
            .get_mut(&task_id)
            .map(|set| set.remove(&dependency_id))
            .unwrap_or(false);

        if let Some(set) = self.dependents.get_mut(&dependency_id) {
            set.remove(&task_id);
        }

        if let Some(task) = self.tasks.get_mut(&task_id) {
            task.dependencies.retain(|&id| id != dependency_id);
        }

        Ok(removed)
    }

    /// Returns all prerequisite tasks that `task_id` directly depends on.
    pub fn dependencies_of(&self, task_id: TaskId) -> Result<Vec<TaskId>, TaskGraphError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(TaskGraphError::UnknownTask(task_id));
        }
        let deps = self
            .dependencies
            .get(&task_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        Ok(deps)
    }

    /// Returns all tasks that directly depend on `task_id`.
    pub fn dependents_of(&self, task_id: TaskId) -> Result<Vec<TaskId>, TaskGraphError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(TaskGraphError::UnknownTask(task_id));
        }
        let dependents = self
            .dependents
            .get(&task_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        Ok(dependents)
    }

    /// Evaluates if a task is ready to run.
    /// A task is ready only when all prerequisite dependencies have Succeeded.
    /// If any dependency is Failed, Cancelled, or unfinished, the task is not ready.
    pub fn is_ready(&self, task_id: TaskId) -> Result<bool, TaskGraphError> {
        let task = self
            .tasks
            .get(&task_id)
            .ok_or(TaskGraphError::UnknownTask(task_id))?;

        if task.state.is_terminal() {
            return Ok(false);
        }

        let deps = self.dependencies.get(&task_id);
        let Some(deps) = deps else {
            return Ok(true);
        };

        for &dep_id in deps {
            let dep_task = self
                .tasks
                .get(&dep_id)
                .ok_or(TaskGraphError::UnknownTask(dep_id))?;
            if dep_task.state != TaskState::Succeeded {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Validates the global structure of the DAG using Kahn's algorithm (topological sorting).
    /// Detects any cycles or orphaned edge references.
    pub fn validate(&self) -> Result<(), TaskGraphError> {
        let mut in_degrees: HashMap<TaskId, usize> = HashMap::new();

        for (&task_id, deps) in &self.dependencies {
            in_degrees.insert(task_id, deps.len());
            for &dep_id in deps {
                if !self.tasks.contains_key(&dep_id) {
                    return Err(TaskGraphError::UnknownTask(dep_id));
                }
            }
        }

        let mut queue: VecDeque<TaskId> = in_degrees
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut visited_count = 0;
        while let Some(task_id) = queue.pop_front() {
            visited_count += 1;
            if let Some(dependents) = self.dependents.get(&task_id) {
                for &dependent_id in dependents {
                    if let Some(degree) = in_degrees.get_mut(&dependent_id) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent_id);
                        }
                    }
                }
            }
        }

        if visited_count != self.tasks.len() {
            return Err(TaskGraphError::MalformedGraph(
                "Graph contains at least one dependency cycle".into(),
            ));
        }

        Ok(())
    }

    /// Iterative BFS to check if `source` can reach `target` following dependencies.
    /// Traversal follows: `current` -> prerequisites that `current` depends on.
    /// If `source` can reach `target`, then `source` already depends on `target`.
    fn reaches_iterative(&self, source: TaskId, target: TaskId) -> bool {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        queue.push_back(source);
        visited.insert(source);

        while let Some(current) = queue.pop_front() {
            if current == target {
                return true;
            }
            if let Some(deps) = self.dependencies.get(&current) {
                for &dep_id in deps {
                    if dep_id == target {
                        return true;
                    }
                    if visited.insert(dep_id) {
                        queue.push_back(dep_id);
                    }
                }
            }
        }

        false
    }

    pub fn get_task(&self, task_id: TaskId) -> Option<&TaskRecord> {
        self.tasks.get(&task_id)
    }

    pub fn get_task_mut(&mut self, task_id: TaskId) -> Option<&mut TaskRecord> {
        self.tasks.get_mut(&task_id)
    }

    pub fn all_tasks(&self) -> impl Iterator<Item = &TaskRecord> {
        self.tasks.values()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_studios_protocol::id::StudioId;
    use chrono::Utc;

    fn make_test_task(studio_id: StudioId, title: &str) -> TaskRecord {
        TaskRecord::new(studio_id, title, "", None, None, vec![], Utc::now())
    }

    #[test]
    fn test_add_task_and_dependencies() {
        let studio_id = StudioId::new();
        let mut graph = TaskGraph::new();

        let t1 = make_test_task(studio_id, "T1");
        let t2 = make_test_task(studio_id, "T2");
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1).unwrap();
        graph.add_task(t2).unwrap();

        graph.add_dependency(id2, id1).unwrap();

        assert_eq!(graph.dependencies_of(id2).unwrap(), vec![id1]);
        assert_eq!(graph.dependents_of(id1).unwrap(), vec![id2]);
    }

    #[test]
    fn test_self_dependency_rejected() {
        let studio_id = StudioId::new();
        let mut graph = TaskGraph::new();
        let t1 = make_test_task(studio_id, "T1");
        let id1 = t1.id;
        graph.add_task(t1).unwrap();

        let err = graph.add_dependency(id1, id1).unwrap_err();
        assert_eq!(err, TaskGraphError::SelfDependency(id1));
    }

    #[test]
    fn test_cycle_detection_simple() {
        let studio_id = StudioId::new();
        let mut graph = TaskGraph::new();
        let t1 = make_test_task(studio_id, "T1");
        let t2 = make_test_task(studio_id, "T2");
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1).unwrap();
        graph.add_task(t2).unwrap();

        graph.add_dependency(id2, id1).unwrap(); // T2 depends on T1
        let err = graph.add_dependency(id1, id2).unwrap_err(); // T1 cannot depend on T2
        assert_eq!(err, TaskGraphError::DependencyCycle { from: id1, to: id2 });
    }

    #[test]
    fn test_cycle_detection_three_nodes() {
        // A -> B -> C -> A
        let studio_id = StudioId::new();
        let mut graph = TaskGraph::new();
        let a = make_test_task(studio_id, "A");
        let b = make_test_task(studio_id, "B");
        let c = make_test_task(studio_id, "C");
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;

        graph.add_task(a).unwrap();
        graph.add_task(b).unwrap();
        graph.add_task(c).unwrap();

        // B depends on A
        graph.add_dependency(id_b, id_a).unwrap();
        // C depends on B
        graph.add_dependency(id_c, id_b).unwrap();
        // A depends on C -> cycle!
        let err = graph.add_dependency(id_a, id_c).unwrap_err();
        assert_eq!(
            err,
            TaskGraphError::DependencyCycle {
                from: id_a,
                to: id_c
            }
        );
    }

    #[test]
    fn test_readiness_logic() {
        let studio_id = StudioId::new();
        let mut graph = TaskGraph::new();
        let mut t1 = make_test_task(studio_id, "T1");
        let t2 = make_test_task(studio_id, "T2");
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1.clone()).unwrap();
        graph.add_task(t2).unwrap();
        graph.add_dependency(id2, id1).unwrap();

        // Initially T1 is ready, T2 is blocked waiting on T1
        assert!(graph.is_ready(id1).unwrap());
        assert!(!graph.is_ready(id2).unwrap());

        // When T1 succeeds, T2 becomes ready
        t1.state = TaskState::Succeeded;
        *graph.get_task_mut(id1).unwrap() = t1;
        assert!(graph.is_ready(id2).unwrap());
    }

    #[test]
    fn test_unsafe_removal_rejected() {
        let studio_id = StudioId::new();
        let mut graph = TaskGraph::new();
        let t1 = make_test_task(studio_id, "T1");
        let t2 = make_test_task(studio_id, "T2");
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1).unwrap();
        graph.add_task(t2).unwrap();
        graph.add_dependency(id2, id1).unwrap();

        let err = graph.remove_task(id1).unwrap_err();
        assert_eq!(
            err,
            TaskGraphError::UnsafeTaskRemoval {
                task_id: id1,
                dependent_count: 1
            }
        );
    }
}
