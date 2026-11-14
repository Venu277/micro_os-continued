//! MicroOS Package Dependency Resolution Engine
//!
//! Implements topological graph sorting to calculate precise, safe installation
//! transaction orders and detect cyclic or unresolvable dependencies.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use crate::pkg::rfc822::PackageMeta;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepError {
    PackageNotFound(String),
    MissingDependency { package: String, depends_on: String },
    CircularDependency(String),
}

impl core::fmt::Display for DepError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DepError::PackageNotFound(pkg) => write!(f, "Package '{}' not found in repository index", pkg),
            DepError::MissingDependency { package, depends_on } => {
                write!(f, "Unmet dependency: '{}' depends on '{}', which is unavailable", package, depends_on)
            }
            DepError::CircularDependency(pkg) => write!(f, "Circular dependency detected involving '{}'", pkg),
        }
    }
}

/// Resolve full dependency graph for a package and return the installation order.
pub fn resolve_dependencies(
    target: &str,
    available: &[PackageMeta],
    installed: &[String],
) -> Result<Vec<PackageMeta>, DepError> {
    let mut order = Vec::new();
    let mut visited = Vec::new();
    let mut in_stack = Vec::new();

    dfs_resolve(
        target,
        available,
        installed,
        &mut order,
        &mut visited,
        &mut in_stack,
    )?;

    Ok(order)
}

fn dfs_resolve(
    current: &str,
    available: &[PackageMeta],
    installed: &[String],
    order: &mut Vec<PackageMeta>,
    visited: &mut Vec<String>,
    in_stack: &mut Vec<String>,
) -> Result<(), DepError> {
    // If already installed, dependency is satisfied
    if installed.iter().any(|p| p == current) {
        return Ok(());
    }

    // If already resolved in this transaction, skip
    if visited.iter().any(|p| p == current) {
        return Ok(());
    }

    // Cycle detection
    if in_stack.iter().any(|p| p == current) {
        return Err(DepError::CircularDependency(current.to_owned()));
    }

    let meta = available
        .iter()
        .find(|p| p.name == current)
        .ok_or_else(|| DepError::PackageNotFound(current.to_owned()))?;

    in_stack.push(current.to_owned());

    for dep in &meta.depends {
        if !installed.iter().any(|p| p == dep) {
            // Check if available in repo
            if !available.iter().any(|p| &p.name == dep) {
                return Err(DepError::MissingDependency {
                    package: current.to_owned(),
                    depends_on: dep.clone(),
                });
            }

            dfs_resolve(dep, available, installed, order, visited, in_stack)?;
        }
    }

    in_stack.pop();
    visited.push(current.to_owned());
    order.push(meta.clone());

    Ok(())
}
