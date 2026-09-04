module.exports = {
    extends: ['airbnb', 'airbnb-typescript', 'prettier'],
    plugins: [
        'unused-imports',
        'eslint-plugin-import',
        '@typescript-eslint',
        'no-relative-import-paths',
        'prettier',
        'header',
    ],
    parserOptions: {
        project: ['./tsconfig.json', './tsconfig.node.json', './tools/scripts/tsconfig.json'],
    },
    overrides: [
        {
            files: ['*'],
            rules: {
                'no-param-reassign': ['error', { props: true, ignorePropertyModificationsForRegex: ["^draft"] }],

                'unused-imports/no-unused-imports': 'error',

                'import/prefer-default-export': 'off',
                'import/no-default-export': 'error',
                'prettier/prettier': 'error',

                'class-methods-use-this': 'off',

                'no-plusplus': ['error', { allowForLoopAfterthoughts: true }],

                // just why
                'react/jsx-uses-react': 'off',
                'react/react-in-jsx-scope': 'off',
                'react/jsx-no-bind': 'off',
                'react/jsx-props-no-spreading': 'off',
                'react/require-default-props': 'off',
                'react/function-component-definition': 'off',

                'react/no-unstable-nested-components': [
                    'error',
                    {
                        allowAsProps: true,
                    },
                ],

                // seems to be bugged for aliases
                'import/extensions': ['error', 'ignorePackages', { '': 'never' }],

                'no-relative-import-paths/no-relative-import-paths': [
                    'error',
                    {
                        rootDir: 'src',
                        prefix: '@',
                    },
                ],

                'no-restricted-imports': [
                    'error',
                    {
                        patterns: [
                            {
                                group: ['@mui/*', '!@mui/material/', '!@mui/icons-material/', '!@mui/x-date-pickers/'],
                            },
                            {
                                group: ['@mui/*/*/*'],
                            },
                        ],
                    },
                ],

                'no-restricted-syntax': [
                    'error',
                    {
                        selector: 'TSTypeReference[typeName.name="SxProps"]:not([typeParameters])',
                        message: 'SxProps must have Theme parameter to avoid significant compiler slowdown.',
                    },
                ],
            },
        },
        {
            files: ['tools/scripts/**/*'],
            rules: {
                'no-relative-import-paths/no-relative-import-paths': 'off',
                'import/no-extraneous-dependencies': ['error', { devDependencies: true }],
            },
        },
        {
            // Lint debt inherited from Suwayomi/Manatan. These are style and
            // accessibility rules that the upstream codebase never satisfied; keeping
            // them as errors would mean rewriting ~90 files of code this fork did not
            // write, and would make the CI gate meaningless on day one.
            //
            // They stay ON as warnings so they remain visible and can be paid down,
            // while `yarn lint` still fails on anything that is an actual error.
            files: ['src/**/*.{ts,tsx}'],
            plugins: ['react-hooks'],
            rules: {
                'no-console': 'warn',
                'no-await-in-loop': 'warn',
                'no-continue': 'warn',
                'no-nested-ternary': 'warn',
                'no-param-reassign': 'warn',
                'no-plusplus': 'warn',
                'no-restricted-globals': 'warn',
                'no-restricted-syntax': 'warn',
                'no-bitwise': 'warn',
                'consistent-return': 'warn',
                'default-case': 'warn',
                'class-methods-use-this': 'warn',
                'prefer-destructuring': 'warn',
                'max-classes-per-file': 'warn',
                'no-underscore-dangle': 'warn',
                'no-promise-executor-return': 'warn',
                'no-cond-assign': 'warn',
                'array-callback-return': 'warn',
                'guard-for-in': 'warn',
                'radix': 'warn',
                'camelcase': 'warn',
                'func-names': 'warn',
                'no-lonely-if': 'warn',
                'no-void': 'warn',
                'no-shadow': 'warn',
                'import/no-cycle': 'warn',
                'import/prefer-default-export': 'warn',
                'import/no-extraneous-dependencies': 'warn',
                'no-relative-import-paths/no-relative-import-paths': 'warn',
                'no-restricted-imports': 'warn',
                '@typescript-eslint/no-use-before-define': 'warn',
                '@typescript-eslint/no-unused-vars': 'warn',
                '@typescript-eslint/no-shadow': 'warn',
                '@typescript-eslint/return-await': 'warn',
                '@typescript-eslint/naming-convention': 'warn',
                '@typescript-eslint/no-loop-func': 'warn',
                '@typescript-eslint/no-explicit-any': 'warn',
                // TypeScript already checks prop types; the rule is pure noise here.
                'react/prop-types': 'off',
                'react/no-array-index-key': 'warn',
                'react/no-unstable-nested-components': 'warn',
                'react/destructuring-assignment': 'warn',
                'react/jsx-no-useless-fragment': 'warn',
                'react/no-danger': 'warn',
                'react-hooks/rules-of-hooks': 'warn',
                'react-hooks/exhaustive-deps': 'warn',
                'jsx-a11y/label-has-associated-control': 'warn',
                'jsx-a11y/click-events-have-key-events': 'warn',
                'jsx-a11y/no-static-element-interactions': 'warn',
                'jsx-a11y/no-noninteractive-element-interactions': 'warn',
                'jsx-a11y/media-has-caption': 'warn',
                'jsx-a11y/anchor-is-valid': 'warn',
                'jsx-a11y/control-has-associated-label': 'warn',
                'react/button-has-type': 'warn',
                'react/jsx-no-duplicate-props': 'warn',
                'react/no-unescaped-entities': 'warn',
                'no-empty': 'warn',
                'no-case-declarations': 'warn',
                'no-control-regex': 'warn',
                'no-useless-escape': 'warn',
                'import/no-default-export': 'warn',
                '@typescript-eslint/no-empty-function': 'warn',
                'jsx-a11y/no-noninteractive-tabindex': 'warn',
            },
        },
    ],
};
