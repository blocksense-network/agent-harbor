# Agent Harbor Specs

This folder contains a work-in-progress specification for the next iteration of the agent-harbor product.

The main ideas of the product are described in the [Product-One-Pager](../docs/Product-One-Pager.md) document. Please read it.

Currently, the spec is not finalized and we are not ready to start the implementation of the software. The folder `specs/Initial Developer Input` contains files that should be treated as a ground truth for the specification effort. You are expected to work on the markdown files in the `specs/Public` folder, which should detail everything with a much higher precision, but please note that not all information in them has been fully reviewed by the development team yet.

Files in the public folder should never refer to documents in the `specs/Initial Developer Input` folder or the `specs/Research` folder, where we put preliminary-research findings that also haven't been fully vetted.

Your goal is to build a very comprehensive specification, meeting the goals stated in the initial developer input and expanding upon them with solid research and engineering. You may use the information provided in the preliminary research findings, but please verify it, potentially by building a small PoC programs.

The public spec should be a stand-alone document that never references other folders. In other words, your job is to transform the content from the other folders into a high-quality spec.

For each file in the `spec/Public` folder, there will be a corresponding file in the `spec/Implementation Progress` folder. This is a place to store information regarding what was already prototyped or implemented for production use. The files in this folder should contain references to source code files that are a good starting point for someone who wants to see the code behind the spec.

Some of the markdown files have standardized Obsidian headers indicating their current review status. Avoid modifying files with status "Reviewed" or "Final" unless explicitly asked.

## Planning the implementation and tracking progress

Implementation efforts have now started on the agent-harbor MVP. Implementation plans break down the work into granular tasks and milestones, each with well-defined success criteria that can be tested with fully automated tests.

The planning and status file typically exists as a separate markdown file with an extension `.status.md` . It may be named after a corresponding spec file. The [MVP.status.md](Public/MVP.status.md) file serves as the primary example for other status.md files, demonstrating the expected structure with clearly specified deliverables and verification criteria based on automated tests.

When a task is complete, the implementation plan should be updated with an implementation status section featuring references to key files that can serve as a good starting point for someone who would like to study the implementation.

When a task proves difficult to complete according to the plan, you should NEVER deviate significantly from the original goal. Instead, you must update the milestone status section with a link to a markdown report detailing what have been tried and what problems were observed. These reports will be forwarded to senior developers and management who may adjust the plan in response. The reports should describe the context of the problem in extreme detail as they may be shared with online AI agents who are not familiar with our project.

### Expected Structure for Status Files

Each milestone in a status.md file should contain:

- **Deliverables:** A bullet-point list of specific features, components, or capabilities that must be implemented
- **Verification:** A bullet-point list of automated test scenarios that validate the deliverables, with emphasis on end-to-end integration tests that demonstrate the feature working in real-world conditions

When milestones are completed, their sections should be expanded with additional documentation:

- **Implementation Details:** Detailed description of architectural decisions, implementation approaches, and key technical insights
- **Key Source Files:** References to specific source files that serve as good starting points for understanding the implementation
- **Outstanding Tasks:** Any remaining work, bugs, or improvements that still need to be addressed
- The **Verification** section should be updated with checkboxes showing which verification criteria have been met (`[x]`) and which are still pending (`[ ]`).

This structure ensures that every milestone has clear, testable completion criteria and that completed work is thoroughly documented for future reference and maintenance.

It's extremely important that the tasks are very granular and that they can be verified with automated tests. Prefer integration tests over unit tests, but apply reasonable judgment on a case-by-case basis.

All implementation plans and testing strategies will be reviewed before the implementation efforts starts. Ideally, the plans will identify various development tracks that can be started in parallel without interfering with each other.

When applicable, plan the creation of reusable Rust crates that can be tested in isolation. We start from the building blocks and then assemble them into larger and larger components.

Don't be shy to propose any specific software or technology for the testing strategy. We are willing to invest a lot of effort in making the described system as robust as possible by creating very sophisticated test harnesses for every single component.

Try to make the milestones and the tasks in the plan as granular as possible. One strategy for this is to first think about the big picture and then to systematically break down the big items into smaller and smaller tasks.

Good luck. Take your time to do this properly.

## Specs Maintenance

- Before committing any change to the `specs/` folder, run `just lint-specs` from the project root. This performs Markdown linting, link checking, spell checking, prose/style linting, and Mermaid diagram validation.

If the pre-commit hook blocks your commit, run `just lint-specs`, address the reported issues, and commit again.

## API Design Guidelines During Spec Development

During the specification design phase, prioritize consistent and clear terminology over backward compatibility concerns (we haven't shipped anything based on this spec yet). API endpoints, message formats, and data structures should use the most accurate and consistent terminology available, even if this means changing names as the spec evolves. This allows for cleaner, more maintainable specifications without being constrained by early naming decisions that may become outdated.

## Mermaid Diagram Guidelines (authoring)

- Use descriptive node IDs (e.g., `are_we_inside_repo`) instead of single letters.
- Quote labels with spaces/parentheses: `["Label (with details)"]`.
- Quote decision nodes that include punctuation: `{"Question?"}`.
- Terminal nodes must have no outgoing edges; name them clearly (e.g., `done`, `exit_error`).
- Close code fences correctly: use proper markdown code fence syntax.

## CLI Documentation Guidelines

### Command Parameter Formatting

When documenting CLI commands with multiple parameters, use the following format to improve readability in Git PRs and general maintainability:

**Good Format:**

```
ah command [OPTIONS] [ARGUMENTS]

DESCRIPTION: The precise user-facing brief description of the command

OPTIONS:
  --option1 <value>           Description of option1
  --option2                   Description of option2
  --long-option-name <type>   Description of long option

ARGUMENTS:
  ARGUMENT1                   Description of required argument
  [ARGUMENT2]                 Description of optional argument
```

**Bad Format (hard to review in PRs):**

```
- `ah command [--option1 <value>] [--option2] [--long-option-name <type>] [argument1] [argument2]`
```

**Benefits of the good format:**

- Each parameter appears on its own line, making diffs cleaner in PRs
- Easier to spot additions, removals, or changes to individual parameters
- Better alignment and readability
- Consistent with standard help screen formatting
- Easier to maintain and update parameter descriptions

**When to use this format:**

- Commands with 3+ parameters
- Complex commands where parameter descriptions are important
- Any command where the single-line format becomes unwieldy

**When single-line format is acceptable:**

- Simple commands with 1-2 parameters
- Commands where brevity is more important than detailed formatting
