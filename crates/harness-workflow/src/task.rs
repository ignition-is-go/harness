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

/// Blanket impl so a single `Arc<T>` can be both `task(...)`ed into a
/// `WorkflowBuilder` and held by a `Sink` (e.g. `PcxSink` reads
/// attributes at record time, the Workflow reads them at consult time).
/// Sharing the Arc avoids cloning bodies that may be many KB of issue
/// text.
impl<T: Task + ?Sized> Task for std::sync::Arc<T> {
    fn id(&self) -> &str {
        (**self).id()
    }
    fn objective(&self) -> &str {
        (**self).objective()
    }
    fn body(&self) -> &str {
        (**self).body()
    }
    fn attributes(&self) -> Attributes {
        (**self).attributes()
    }
}
