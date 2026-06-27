# Test-Driven Development

- Write a test for any non-trivial logic, alongside (or before) the
  implementation. Tests are the executable specification.
- Cover the real edge cases: empty input, malformed input, boundary values, and
  each distinct error path — assert the specific error, not just "it failed".
- A change is not done until its tests are green. Never claim success without
  running the verification and confirming the output.
- When fixing a bug, first add a test that reproduces it (red), then fix it
  (green). The test prevents the regression from returning.
