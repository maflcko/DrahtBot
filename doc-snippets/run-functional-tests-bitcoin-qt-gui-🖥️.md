https://github.com/bitcoin/bitcoin/pull/12873/commits/85e2b57749590df7a3c93619d9fde0e856bfc555

```
QT_FATAL_WARNINGS=0 LC_ALL=de_DE.UTF-8 QT_QPA_PLATFORM=offscreen BITCOIND=bitcoin-qt ./bld-cmake/test/functional/test_runner.py -j 2
```

```diff
diff --git a/test/functional/test_framework/test_node.py b/test/functional/test_framework/test_node.py
index a7c140a53b..a016eb7d3f 100755
--- a/test/functional/test_framework/test_node.py
+++ b/test/functional/test_framework/test_node.py
@@ -671,2 +671,3 @@ class TestNode():
         assert not self.running
+        return
         with tempfile.NamedTemporaryFile(dir=self.stderr_dir, delete=False) as log_stderr, \

