import unittest

from aira_graphdb_sdk import (
    create_handshake_request,
    load_error_contract,
    load_typemap_contract,
    map_known_error,
)


class GraphDbSdkTests(unittest.TestCase):
    def test_load_typemap_contract(self) -> None:
        contract = load_typemap_contract()
        self.assertEqual(contract["spec_id"], "AGDB-TYPEMAP-P0@1.0.0")

    def test_create_handshake_request(self) -> None:
        payload = create_handshake_request("protocol-p0@1.0.0", "canonical-types@1.0.0")
        self.assertEqual(payload["protocol_version"], "protocol-p0@1.0.0")

    def test_map_known_error(self) -> None:
        known = map_known_error("AUTH_FAILED", "x")
        self.assertEqual(known["code"], "AUTH_FAILED")
        unknown = map_known_error("X_UNKNOWN", "x")
        self.assertEqual(unknown["code"], "UNSUPPORTED_FEATURE")
        err_contract = load_error_contract()
        self.assertEqual(err_contract["spec_id"], "AGDB-ERROR-CODES@1.0.0")


if __name__ == "__main__":
    unittest.main()
