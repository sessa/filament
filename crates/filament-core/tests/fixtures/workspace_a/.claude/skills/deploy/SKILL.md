---
name: deploy
description: Deploy the application to production with safety checks.
allowed-tools: Bash, Read
argument-hint: "[environment]"
---

# Deploy

1. Run the test suite.
2. Build the release artifact.
3. Push to the target environment ($ARGUMENTS).
