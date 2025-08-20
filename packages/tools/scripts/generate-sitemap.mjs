#!/usr/bin/env node

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Generate sitemap from source file
async function generateSitemap() {
  try {
    // Parse from source (dist import is complex with ES modules and TypeScript)
    const toolsPath = path.join(__dirname, '../src/tools.ts');
    const toolsContent = fs.readFileSync(toolsPath, 'utf-8');
    
    // Extract tool paths using regex
    const pathMatches = toolsContent.match(/path: ["']([^"']+)["']/g);
    
    if (!pathMatches) {
      throw new Error('Could not extract tool paths from tools.ts');
    }
    
    const tools = pathMatches.map(match => ({
      path: match.match(/path: ["']([^"']+)["']/)[1]
    }));

    // Generate sitemap XML
    const baseUrl = 'https://tools.mqlang.org';
    const currentDate = new Date().toISOString().split('T')[0];

    let sitemap = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>${baseUrl}/</loc>
    <lastmod>${currentDate}</lastmod>
    <changefreq>monthly</changefreq>
    <priority>1.0</priority>
  </url>`;

    // Add tool pages
    if (!tools || tools.length === 0) {
      throw new Error('No tools found');
    }
    
    tools.forEach(tool => {
      sitemap += `
  <url>
    <loc>${baseUrl}${tool.path}</loc>
    <lastmod>${currentDate}</lastmod>
    <changefreq>monthly</changefreq>
    <priority>0.8</priority>
  </url>`;
    });

    sitemap += `
</urlset>`;

    // Ensure public directory exists
    const publicDir = path.join(__dirname, '../public');
    if (!fs.existsSync(publicDir)) {
      fs.mkdirSync(publicDir, { recursive: true });
    }

    // Write sitemap to public directory
    const sitemapPath = path.join(publicDir, 'sitemap.xml');
    fs.writeFileSync(sitemapPath, sitemap);

    console.log(`Sitemap generated successfully at ${sitemapPath}`);
    console.log(`Generated ${tools.length + 1} URLs:`);
    console.log('- /');
    tools.forEach(tool => console.log(`- ${tool.path}`));
    
  } catch (error) {
    console.error('Error generating sitemap:', error);
    process.exit(1);
  }
}

generateSitemap();