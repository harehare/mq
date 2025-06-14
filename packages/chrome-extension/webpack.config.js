const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const CopyWebpackPlugin = require('copy-webpack-plugin');

module.exports = {
  entry: {
    popup: './popup.tsx', // Assuming popup.tsx is in the root of packages/chrome-extension
    content: './content.ts', // Assuming content.ts is in the root
    background: './background.ts' // Assuming background.ts is in the root
  },
  output: {
    path: path.resolve(__dirname, 'dist'),
    filename: '[name].js',
  },
  resolve: {
    extensions: ['.ts', '.tsx', '.js']
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: 'ts-loader',
        exclude: /node_modules/,
      },
      {
        test: /\.css$/,
        use: ['style-loader', 'css-loader'],
      },
    ],
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: './popup.html', // Assuming popup.html is in the root
      filename: 'popup.html',
      chunks: ['popup'], // Only include popup.js in popup.html
    }),
    new CopyWebpackPlugin({
      patterns: [
        { from: 'manifest.json', to: 'manifest.json' },
        // Add any other static assets here if needed, e.g., icons
        // { from: 'icons/*', to: 'icons/[name][ext]' },
      ],
    }),
  ],
  // Optional: if you want to disable webpack's default performance hints
  performance: {
    hints: false,
  },
  // Optional: if you want to see more detailed error messages
  // stats: 'errors-warnings',
};
