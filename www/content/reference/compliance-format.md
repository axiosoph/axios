+++
title = "Compliance Annotation Format & Schema"
description = "Reference guide for the Axios codebase compliance annotation syntax and JSON database schema"
+++

**Metadata:**

- **Quadrant:** Reference
- **Audience:** Developers adding code annotations or spec writers defining constraints

# Compliance Annotation Format & Schema

This reference page defines the codebase compliance annotation syntax, the JSON schema for tracking data, and instructions for annotating new implementations.

## Codebase Annotation Syntax

To declare that a specific line or block of code complies with a specification constraint, add a comment block directly preceding the implementation:

```rust
// @spec-compliance[<constraint-id>]
// Mechanism: <Description of how the constraint is enforced in this codebase location>
// Verified-By: <Relative path to testing file>:<Test name or function name>
```

### Syntax Fields

| Field Name        | Description                                                                                       | Example                                                       |
| :---------------- | :------------------------------------------------------------------------------------------------ | :------------------------------------------------------------ |
| `<constraint-id>` | The normalized identifier of the constraint as defined in the specification's verification table. | `lock-dag-acyclicity`                                         |
| `Mechanism`       | A clear, brief description of the code mechanism that enforces this constraint.                   | `DFS cycle check over requires graph`                         |
| `Verified-By`     | The relative path to the test file and the test function name validating the behavior.            | `ion/ion-lock/src/lib.rs:test_lock_file_acyclicity_invariant` |

## JSON Database Schema

The compliance tracking tool outputs results to `docs/compliance.json`. The schema of this file maps constraint IDs to their status and verification paths.

### Schema Template

```json
{
  "constraints": {
    "<constraint-id>": {
      "status": "VERIFIED" | "UNVERIFIED",
      "specification_file": "<path/to/spec.md>",
      "mechanism": "<description>" | null,
      "verification_paths": [
        {
          "code_path": "<path/to/implementation.rs>",
          "line_number": <integer>,
          "test_path": "<path/to/test.rs>:<test_name>" | null
        }
      ]
    }
  }
}
```

### JSON Property Descriptions

- **`constraints`** — Root object containing all extracted constraints keyed by their display ID.
- **`status`** — `"VERIFIED"` if at least one annotation exists in the codebase; `"UNVERIFIED"` otherwise.
- **`specification_file`** — The relative repository path of the specification declaring the constraint.
- **`mechanism`** — Concatentated mechanism descriptions from all matching code annotations (or `null` if unverified).
- **`verification_paths`** — List of implementation details for the constraint (empty array if unverified).
  - **`code_path`** — Relative repository path to the Rust file containing the compliance annotation.
  - **`line_number`** — The line number of the `@spec-compliance` tag (1-indexed).
  - **`test_path`** — The test file and function name verifying this implementation.
