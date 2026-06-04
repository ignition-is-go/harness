use super::outcome::Attributes;

/// What needs doing. The unit of work the Workflow runs to completion.
///
/// Implementations carry whatever the source system gave them
/// (a GitHub issue, a Linear ticket, a CVE advisory, a synthesized
/// improvement proposal) and expose the four things the Workflow
/// needs to drive a run:
///
/// - `id` — stable identifier across runs, used as the outcome key
/// - `objective` — short LLM-facing problem statement
/// - `body` — full context the consult step gets verbatim
/// - `attributes` — metadata for the outcome record (subject.*,
///   topic.*, etc.)
pub trait Task: Send + Sync {
    fn id(&self) -> &str;
    fn objective(&self) -> &str;
    fn body(&self) -> &str;
    fn attributes(&self) -> Attributes;
}
