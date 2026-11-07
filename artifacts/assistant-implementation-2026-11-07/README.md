# Selected assistant implementation evidence

The [implementation report](implementation-report.md) records the changes, exact
validation scope and outstanding release gates. [Build identity](final-verification.json)
and [final native smoke](final-native-smoke.json) identify the tested development build.
Historical evidence includes selected native screenshots/UIA records, receipt/response
records, effect/export checks and the repeated model-task matrix.

The repository retains only paths explicitly listed in `artifacts/.gitignore`, following
the [repository policy](../../docs/repository-layout.md). Patch backups, process sessions,
profiles, disposable notebooks, runtime databases, intermediate logs, executables and
exported sample files remain local. References to those local files in the historical
report describe the test setup; they are not dependencies of a fresh clone. Test suites
create their own fixtures. No credentials or raw reasoning traces are part of this set.
Credential-shaped synthetic parameters in XML test names are replaced with a fixture
marker for publication; test outcomes, counts and timings are unchanged. Original XML
and the repository-integration audit/backups stay local.
