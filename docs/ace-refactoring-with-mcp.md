# ACE refactoring with MCP

CodeScene MCP works well on its own for refactoring guidance through Code Health reviews, scores, and safeguards.

If your team also uses CodeScene ACE, you can combine it with MCP to speed up early restructuring on some large or complex functions.

## When ACE helps

ACE is useful when a function is too large or tangled for reliable stepwise AI refactoring. In those cases, ACE can generate an initial split into smaller and more cohesive units, after which normal MCP-guided refactoring can continue.

## Typical workflow

1. Use MCP Code Health review tools to identify unhealthy functions.
2. Optionally run ACE auto-refactor for a complex function.
3. Continue with small manual or AI-assisted refactoring steps.
4. Re-run Code Health tools to verify no regressions and measurable improvement.

## Requirements

- ACE is a CodeScene add-on and requires a separate license.
- See [Configuration Options](configuration-options.md#ace_access_token) for setup details.

Request ACE access and licensing details via the [CodeScene contact page](https://codescene.com/contact-us-about-codescene-ace).
