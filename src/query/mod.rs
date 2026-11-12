//! Queries over the library: saved views, each with a nested filter, sorts and an optional
//! group-by. With no filters, sorts or grouping a view is the whole library in library order.
//! The design follows Obsidian's Bases.
//!
//! - [`property`]: the song attributes views read, and their values.
//! - [`filter`]: conditions (property, operator, value) in all/any/none groups, nested.
//! - [`view`]: a view's settings, and evaluating one into ordered, grouped rows.
//! - [`store`]: the views saved as JSON in the app's config folder.

pub mod filter;
pub mod property;
pub mod store;
pub mod view;

#[cfg(test)]
mod tests;
