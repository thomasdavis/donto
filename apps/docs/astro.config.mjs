import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://thomasdavis.github.io',
  base: '/donto',
  integrations: [
    starlight({
      title: 'donto',
      description: 'Bitemporal paraconsistent quad store on Postgres + Lean 4',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/thomasdavis/donto' },
      ],
      sidebar: [
        {
          label: 'Getting Started',
          autogenerate: { directory: 'getting-started' },
        },
        {
          label: 'Guides',
          autogenerate: { directory: 'guides' },
        },
        {
          label: 'Reference',
          autogenerate: { directory: 'reference' },
        },
        {
          label: 'Lean Verification',
          autogenerate: { directory: 'lean' },
        },
        {
          label: 'Architecture',
          autogenerate: { directory: 'architecture' },
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/thomasdavis/donto/edit/main/apps/docs/',
      },
    }),
  ],
});
