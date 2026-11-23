import unittest

from tools.test_server_resource_pack_parity import vanilla_config_view


class ServerResourcePackTranscriptTests(unittest.TestCase):
    def test_projects_only_the_configuration_handshake_slot(self):
        brand = 'Alice config custom_payload minecraft:brand'
        features = 'Alice config update_enabled_features'
        build = 'Alice config custom_payload horizon:build'
        query = 'Alice config custom_payload horizon:payloads'
        packs = 'Alice config select_known_packs'
        outcome = ['Alice config registry_data x32', 'Alice disconnected in configuration: rejected']
        baseline = [brand, features, packs, *outcome]
        self.assertEqual(vanilla_config_view([brand, features, build, query, packs, *outcome]), baseline)

        for extra in ('Alice config custom_payload minecraft:other',
                      'Alice config custom_payload horizon:other',
                      'Alice play custom_payload horizon:payloads',
                      'Alice config custom_payload horizon:payloads x2',
                      'Bob config custom_payload horizon:payloads'):
            with self.subTest(extra=extra):
                trace = [brand, features, build, extra, packs, *outcome]
                self.assertEqual(vanilla_config_view(trace), trace)

        for trace in ([brand, features, packs, build, query, *outcome],
                      [brand, features, query, build, packs, *outcome],
                      [brand, features, build, query, query, packs, *outcome]):
            with self.subTest(trace=trace):
                self.assertEqual(vanilla_config_view(trace), trace)
