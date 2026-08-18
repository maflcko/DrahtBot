# How to reduce a compiler bug

In practise, a compiler bug will usually manifest inside a large executable, possibly a fuzz test, unit test, or the main program.

To investigate, possibly report and fix the bug, it should be reduced. All reduction steps must preserve the bug.

A compilation error bug is usually easier to reduce, because normally only a single translation unit is needed, without the need to run a link step.

Should the compiler spit out the wrong assembly, leading to runtime errors, a main function and a successful link step is needed.

Usually, automated tools such as cvise/creduce can be used to minimize, but they often end up in an ugly local minima, sometimes with added UB, that is hard to recover from.

Thus, it may be faster and cleaner to manually reduce with the following unordered steps in a loop:

* Remove unused functions/logic/variables/classes from the affected translation unit(s)
* Inline anything that is only used once (a function, a struct, a project-include directive)
* Simplify logic (e.g. remove lines), simplify types (e.g. move toward C++11 types or even raw-c-arrays), ...

## reproduction script

The script is written for cvise/creduce, but is also helpful for a human (or llm) reducer:

```sh
#!/bin/bash

# the file must be hard-coded for cvise/creduce here
# Call either `cvise ./this_script.sh work.cc`
# or manually just call `./this_script.sh`
# in both cases, the work.cc must sit in PWD.
SOURCE_FILE="work.cc"
OUTPUT_EXE="./tmp_exe"

# Compilers
GCC15="ccache g++-15"
GCC16="ccache g++-16"

# Minimum necessary args for the bug to manifest. Include errors to reject possible UB early!
COMMON_ARGS="-O2 -std=c++17 -g1  -Wall   -Werror=return-type -Werror=uninitialized -Werror=array-bounds"

# --- Compile and Run with GCC 15 (The Reference Version) ---
if ! $GCC15 $COMMON_ARGS -o $OUTPUT_EXE "$SOURCE_FILE" ; then
    echo "Compilation failed with GCC 15. Undo the last change."
    exit 1
fi

# optionally run (only for runtime bugs):
# include a meaningful timeout to avoid zombie valgrind processes eating the CPU
timeout 4s valgrind --tool=none --error-exitcode=43 --exit-on-first-error=yes "$OUTPUT_EXE"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
    echo "FAILURE: GCC 15 returned $EXIT_CODE (Expected 0). Undo the last change."
    exit 1
fi

#  Compile and Run with GCC 16 (The Buggy Version) ---
if ! $GCC16 $COMMON_ARGS -o $OUTPUT_EXE "$SOURCE_FILE" ; then
    echo "Compilation failed with GCC 16. Undo the last change."
    exit 1
fi

timeout 4s valgrind --tool=none --error-exitcode=43 --exit-on-first-error=yes "$OUTPUT_EXE" 
EXIT_CODE=$?
if [ $EXIT_CODE -ne 139 ]; then
    echo "FAILURE: GCC 16 returned $EXIT_CODE (Expected segfault). Undo the last change."
    exit 1
fi

# --- Success ---
echo "All validation steps complete. Continue with the next change"
exit 0

```

# LLM prompt

With recent 2026 models, LLMs can be used to complement a human reducer. A possible prompt could be:

```
You are an expert compiler bug code reducer. Start by running the
`/tmp/c.sh`
test script. The script will compile and then either print that the issue still reproduces and the next change can be tried, or that the issue went away and the last change must be undone.
Then make minimal changes to the code, to reduce it. Get the source code file name from the test script.

After each change, run the test script to confirm. Then either revert the last change, or continue with the next. As a start idea, you can try to make the code logically more meaningful or simple. Try to move towards something that a human could have written, and not something that looks like a creduce output.

* Remove unused functions/logic/variables/classes from the affected translation unit(s)
* Inline anything that is only used once (a function, a struct, a project-include directive)
* Simplify logic (e.g. remove lines), simplify types (e.g. move from advanced and modern std lib language features toward raw low-level, or older features or collections), ...


```
