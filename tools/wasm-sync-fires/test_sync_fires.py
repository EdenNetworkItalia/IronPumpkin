import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import sync_fires  # noqa: E402

HOST_SOURCE = """
impl HostPlayer for PluginHostState {
    fn direct(&mut self) -> wasmtime::Result<()> {
        // fire_blocking(); in a comment is no call
        let text = "fire_blocking()";
        open_sign(text);
        Ok(())
    }

    fn deferred(&mut self) -> wasmtime::Result<()> {
        self.defer("label", move || open_sign(""))
    }

    fn quiet(&mut self) -> wasmtime::Result<char> {
        Ok('{')
    }
}
"""

CORE_SOURCE = """
pub fn open_sign(text: &str) {
    server.plugin_manager.fire_blocking(&server, &mut event);
}

pub async fn later() {
    server.plugin_manager.fire_blocking(&server, &mut event);
}
"""


class ScanTest(unittest.TestCase):
    def scan(self, host, core):
        with tempfile.TemporaryDirectory() as root:
            for directory, source in ((sync_fires.HOST, host), ("crates/pumpkin-core/src", core)):
                os.makedirs(os.path.join(root, directory))
                with open(os.path.join(root, directory, "lib.rs"), "w", encoding="utf-8") as file:
                    file.write(source)
            return sync_fires.scan(root)

    def test_a_direct_fire_is_found_and_deferred_work_is_not(self):
        found = self.scan(HOST_SOURCE, CORE_SOURCE)
        self.assertEqual(sorted(found), ["HostPlayer.direct"])
        self.assertEqual(found["HostPlayer.direct"][2], [["open_sign", "fire_blocking"]])

    def test_each_path_of_an_import_is_a_chain(self):
        host = HOST_SOURCE.replace("open_sign(text);", "open_sign(text);\n        read_level();")
        core = CORE_SOURCE + "\npub fn read_level() {\n    stack.get_enchantment_level(e);\n}\n"
        found = self.scan(host, core)
        self.assertEqual(
            found["HostPlayer.direct"][2],
            [["open_sign", "fire_blocking"], ["read_level", "get_enchantment_level"]],
        )

    def test_a_deferring_send_is_no_path(self):
        core = CORE_SOURCE.replace(
            "server.plugin_manager.fire_blocking(&server, &mut event);",
            "client.try_enqueue_packet_data(data);",
            1,
        ) + "\npub fn try_enqueue_packet_data(d: u8) {\n    fire_blocking();\n}\n"
        self.assertEqual(self.scan(HOST_SOURCE, core), {})

    def test_an_async_import_outside_run_blocking_is_a_path(self):
        host = HOST_SOURCE + """
impl HostPlayerWithStore<PluginHostState> for HasSelf<PluginHostState> {
    async fn direct(accessor: &Accessor) -> wasmtime::Result<()> {
        open_sign("");
        Ok(())
    }

    async fn wrapped(accessor: &Accessor) -> wasmtime::Result<()> {
        run_blocking(accessor, move || open_sign("")).await
    }
}
"""
        found = self.scan(host, CORE_SOURCE)
        self.assertIn("async HostPlayer.direct", found)
        self.assertNotIn("async HostPlayer.wrapped", found)

    def test_an_async_function_is_no_path(self):
        found = self.scan(HOST_SOURCE.replace("open_sign(text)", "later()"), CORE_SOURCE)
        self.assertEqual(found, {})

    def test_strip_keeps_the_offsets(self):
        src = "let a = \"}\"; // }\nlet b = '}'; /* } */ let c = r#\"}\"#;"
        stripped = sync_fires.strip(src)
        self.assertEqual(len(stripped), len(src))
        self.assertNotIn("}", stripped)


if __name__ == "__main__":
    unittest.main()
