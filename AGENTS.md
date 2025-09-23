# Testing your changes

- You can execute the test suite with `just test`.
- You can lint the codebase with `just lint`.
  Don't disable lints just to make the linter happy. ALWAYS try to fix the code first.

## 🧪 Testing Tips

When the test suite fails and you want to test potential fixes, try running only the affected
tests firsts:

`ruby -Itest test/test_file.rb -n test_method_name`

## Code quality guidelines

- ALWAYS strive to achieve high code quality.
- ALWAYS write secure code.
- ALWAYS make sure the code is well tested and edge cases are covered. Design the code for testability and be extremely thorough.
- ALWAYS write defensive code and make sure all potential errors are handled.
- ALWAYS strive to write highly reusable code with routines that have high fan in and low fan out.
- ALWAYS keep the code DRY.
- Aim for low coupling and high cohesion. Encapsulate and hide implementation details.
- When creating executable, ALWAYS make sure the functionality can also be used as a library.
  To achieve this, avoid global variables, raise/return errors instead of terminating the program, and think whether the use case of the library requires more control over logging
  and metrics from the application that integrates the library.

## Code commenting guidelines

- Document public APIs and complex modules using standard code documentation conventions.
- Comment the intention behind you code extensively. Omit comments only for very obvious
  facts that almost any developer would know.
- Maintain the comments together with the code to keep them meaningful and current.
- When the code is based on specific formats, standards or well-specified behavior of
  other software, always make sure to include relevant links (URLs) that provide the
  necessary technical details.

### Test writing guidelines

- Each test MUST create a unique log file capturing its full output.
- On success: tests print minimal output to keep logs out of AI context windows.
- On failure: the test runner prints the path and file size of the relevant log(s) so developers (or agents) can open them directly without flooding the console or context.
- Rationale: preserves context-budget for AI tools by avoiding large inline logs, while retaining full fidelity in files.
- NEVER cheat with the tested assertions in order to satisfy a specified Verification requirement (e.g. one from a .status.md file)

## Writing git commit messages

- You MUST use multiline git commit messages using heredoc syntax.
- Use the conventional commits style for the first line of the commit message.
- Use the summary section of your final response as the remaining lines in the commit message.
