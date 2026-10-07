import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import tsEslint from "typescript-eslint";

export default [
	js.configs.recommended,
	...tsEslint.configs.recommended,
	prettier,
	...svelte.configs.prettier,
	{
		files: ["**/*.svelte"],
		languageOptions: {
			ecmaVersion: 2022,
			sourceType: "module",
			globals: {
				...globals.browser,
				...globals.node,
				Bun: "readonly",
			},
			parser: svelte.parser,
			parserOptions: {
				parser: tsEslint.parser,
				extraFileExtensions: [".svelte"],
			},
		},
		rules: {
			"no-useless-assignment": "off",
		},
	},
	{
		files: ["plugins/**/assets/propertyInspector/*.js"],
		languageOptions: {
			globals: globals.browser,
		},
	},
	{
		rules: {
			"@typescript-eslint/ban-ts-comment": "off",
			"@typescript-eslint/no-explicit-any": "off",
			"@typescript-eslint/no-unused-vars": [
				"error",
				{
					argsIgnorePattern: "^_",
					caughtErrorsIgnorePattern: "^_",
					varsIgnorePattern: "^_",
				},
			],
		},
	},
	{
		ignores: ["node_modules/**", "build/**", ".svelte-kit/**", "src-tauri/**", "plugins/**/src/**", "test-results/**", "playwright-report/**"],
	},
];
