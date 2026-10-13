#!/usr/bin/env python3
"""Synthetic Git fixture tests only. Never runs or changes Jackin source."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('collector', Path(__file__).with_name('collect_git_evidence.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)

class CollectorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / 'repo'
        self.repo.mkdir()
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Synthetic fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        (self.repo / 'file.txt').write_text('base\n')
        self.git('add', 'file.txt'); self.git('commit', '-m', 'base')
        self.target = self.git('rev-parse', 'HEAD')

    def git(self, *args):
        p = subprocess.run(['git', '-C', str(self.repo), *args], check=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return p.stdout.decode().strip()

    def test_aliases_equivalent_patches_and_dirty_work_preserved(self):
        shas = []
        for branch in ['feature-a', 'feature-b']:
            self.git('checkout', '-b', branch, self.target)
            (self.repo / 'file.txt').write_text('base\nfeature\n')
            self.git('add', 'file.txt'); self.git('commit', '-m', branch)
            shas.append(self.git('rev-parse', 'HEAD'))
        self.git('branch', 'alias', shas[0])
        self.git('update-ref', 'refs/remotes/origin/main', self.target)
        self.git('remote', 'add', 'origin', 'https://name:secret@example.invalid/repo.git?token=secret')
        (self.repo / 'untracked.txt').write_bytes(b'preserve me')
        before = self.git('status', '--porcelain')
        report = c.collect(self.repo, self.target, self.base / 'evidence', 2, True, True)
        self.assertEqual(5, len(report['branches']))
        self.assertEqual(3, len(report['tip_summaries']))
        self.assertEqual(3, len(report['pairwise']))
        self.assertTrue(any(set(shas).issubset(set(group)) for group in report['patch_id_groups'].values()))
        self.assertTrue(report['worktree_status_unchanged'])
        self.assertEqual(before, self.git('status', '--porcelain'))
        self.assertEqual(b'preserve me', (self.repo / 'untracked.txt').read_bytes())
        self.assertNotIn('secret', json.dumps(report['remotes']))

    def test_output_inside_repo_rejected(self):
        with self.assertRaises(c.EvidenceError):
            c.collect(self.repo, self.target, self.repo / 'output')
        self.assertFalse((self.repo / 'output').exists())

    def test_existing_output_not_overwritten(self):
        out = self.base / 'existing'; out.mkdir()
        (out / 'important').write_text('keep')
        with self.assertRaises(c.EvidenceError):
            c.collect(self.repo, self.target, out)
        self.assertEqual('keep', (out / 'important').read_text())

    def test_renames_record_both_paths(self):
        self.git('mv', 'file.txt', 'renamed.txt'); self.git('commit', '-m', 'rename')
        tip = self.git('rev-parse', 'HEAD')
        report = c.collect(self.repo, self.target, self.base / 'renames')
        record = json.loads((self.base / 'renames' / 'tips' / f'{tip}.json').read_text())
        self.assertTrue(any(x.get('destination') == 'renamed.txt' and x['path'] == 'file.txt'
                            for x in record['target_to_tip_files']))

    def test_linked_worktree_common_git_directory_rejected(self):
        worktree = self.base / 'linked'
        self.git('worktree', 'add', '-b', 'linked', str(worktree), self.target)
        out = self.repo / '.git' / 'do-not-write'
        with self.assertRaises(c.EvidenceError):
            c.collect(worktree, self.target, out)
        self.assertFalse(out.exists())

    def test_multi_parent_tip_retains_all_parent_trees(self):
        tree = self.git('rev-parse', 'HEAD^{tree}')
        parents = [self.target]
        for n in range(2):
            parents.append(self.git('commit-tree', tree, '-p', self.target, '-m', f'parent-{n}'))
        args = ['commit-tree', tree, '-m', 'synthetic multiparent fixture']
        for parent in parents:
            args.extend(['-p', parent])
        tip = self.git(*args)
        self.git('update-ref', 'refs/heads/multiparent', tip)
        out = self.base / 'multiparent'
        c.collect(self.repo, self.target, out)
        row = json.loads((out / 'tips' / f'{tip}.json').read_text())
        self.assertEqual(set(parents), set(row['multi_parent_trees']))
        self.assertEqual(3, len(row['parent_deltas']))

    def test_ref_change_marks_snapshot_unstable(self):
        real = c.tree_manifest
        def change_ref(git, sha):
            self.git('branch', 'concurrent-change', self.target)
            return real(git, sha)
        with patch.object(c, 'tree_manifest', side_effect=change_ref):
            report = c.collect(self.repo, self.target, self.base / 'changed')
        self.assertFalse(report['refs_unchanged'])
        self.assertTrue(any(x.startswith('REFS_CHANGED') for x in report['errors']))

    def test_shallow_history_is_not_complete_evidence(self):
        # A shallow file marks the synthetic root as a boundary without a network clone.
        (self.repo / '.git' / 'shallow').write_text(self.target + '\n')
        report = c.collect(self.repo, self.target, self.base / 'shallow-evidence')
        self.assertTrue(report['shallow'])
        self.assertTrue(any(x.startswith('SHALLOW_HISTORY') for x in report['errors']))

if __name__ == '__main__':
    unittest.main(verbosity=2)
