import path from 'node:path';
import { defineConfig } from '@rspack/cli';
import { rspack } from '@rspack/core';
import sveltePreprocess from 'svelte-preprocess';

export default defineConfig((_env, argv) => {
  const production = argv.mode === 'production';

  return {
    entry: {
      main: './src/main.ts',
    },
    output: {
      path: path.resolve('dist'),
      filename: production ? '[name].[contenthash].js' : '[name].js',
      clean: true,
    },
    resolve: {
      extensions: ['.mjs', '.js', '.ts', '.svelte'],
      mainFields: ['svelte', 'browser', 'module', 'main'],
      conditionNames: ['svelte', 'browser', 'import', '...'],
    },
    module: {
      rules: [
        {
          test: /\.ts$/,
          exclude: /node_modules/,
          use: [
            {
              loader: 'builtin:swc-loader',
              options: {
                jsc: {
                  parser: {
                    syntax: 'typescript',
                  },
                },
              },
            },
          ],
          type: 'javascript/auto',
        },
        {
          test: /\.svelte$/,
          use: [
            {
              loader: 'svelte-loader',
              options: {
                compilerOptions: {
                  dev: !production,
                  css: 'injected',
                },
                emitCss: false,
                hotReload: !production,
                preprocess: sveltePreprocess({
                  sourceMap: !production,
                }),
              },
            },
          ],
        },
      ],
    },
    plugins: [
      new rspack.HtmlRspackPlugin({
        template: './index.html',
      }),
    ],
    devtool: production ? false : 'source-map',
    devServer: {
      host: '127.0.0.1',
      port: 1420,
      hot: true,
      historyApiFallback: true,
      client: {
        overlay: true,
      },
      devMiddleware: {
        writeToDisk: false,
      },
    },
  };
});
