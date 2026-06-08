import { expect, test } from 'bun:test';

test('source install docs name the workspace package', async () => {
  const doc = await Bun.file('content/docs/getting-started/installation.mdx').text();
  const command = 'cargo install --git https://github.com/jackin-project/jackin.git jackin --locked';

  expect(doc).toContain(`\`\`\`bash\n${command}\n\`\`\``);
});
