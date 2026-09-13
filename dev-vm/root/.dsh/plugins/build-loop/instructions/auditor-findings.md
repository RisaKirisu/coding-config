# Auditor finding policy

These rules govern finding selection and reporting for both auditors. The caller decides fix or ignore; the builder only receives approved fixes.

Internally score every candidate finding from 0 to 100. Confidence means how strongly inspected evidence establishes that a defect or concrete maintainability problem exists and applies under the current requirements and supported conditions. Confidence is not impact, preference, or the severity of a hypothetical consequence.

| Confidence | Evidence standard |
| --- | --- |
| 0–25 | Speculation without supporting inspection. |
| 26–50 | Plausible concern with a central premise unverified. |
| 51–75 | Supporting evidence exists, but material uncertainty remains. |
| 76–90 | Requirements, relevant conditions, and the problem are established without an unverified central premise. |
| 91–100 | Direct reproduction or an unambiguous contradiction established from inspected code, tests, or requirements. |

Report a finding only when confidence is strictly greater than 75; 75 is excluded. Any material unverified premise caps confidence at 75. High impact does not raise confidence. Concrete maintainability problems and missing test observations can qualify through inspection; runtime reproduction is not mandatory. Return impact (high, medium, or low), location, rule, evidence, and correction; keep the confidence score internal. If missing evidence prevents completion of the assigned audit, use blocked rather than inventing a finding or claiming completion. When the audit completes normally and is not blocked, omit the blocked property from the JSON report (do not output 'blocked': "").

Return every assigned active finding ID in prior as resolved or open. Ignored findings and caller reasons are context, not a permanent prohibition. Leave them omitted unless stronger evidence merits reopening; then return the same ID as open and explain specifically what new evidence overcomes the caller’s reason. Apply the same greater-than-75 threshold. The caller decides again. Do not restate ignored findings as new IDs, repeat the same argument, or re-justify every ignored item.
