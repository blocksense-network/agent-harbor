import express from 'express';

const router = express.Router();

interface Repository {
  id: string;
  name: string;
  branch: string;
  lastCommit: string;
  url?: string;
}

// Mock repository data
const mockRepositories: Repository[] = [
  {
    id: '1',
    name: 'agents-workflow-webui',
    branch: 'main',
    lastCommit: 'feat: Add real-time session updates',
    url: 'https://github.com/example/agents-workflow-webui.git',
  },
  {
    id: '2',
    name: 'agents-workflow-core',
    branch: 'develop',
    lastCommit: 'refactor: Improve API error handling',
    url: 'https://github.com/example/agents-workflow-core.git',
  },
  {
    id: '3',
    name: 'agents-workflow-cli',
    branch: 'main',
    lastCommit: 'fix: Resolve path resolution issues',
    url: 'https://github.com/example/agents-workflow-cli.git',
  },
  {
    id: '4',
    name: 'agents-workflow-docs',
    branch: 'main',
    lastCommit: 'docs: Update API documentation',
    url: 'https://github.com/example/agents-workflow-docs.git',
  },
];

// GET /api/v1/repositories - List repositories
router.get('/', (req, res) => {
  res.json({
    items: mockRepositories,
    pagination: {
      page: 1,
      perPage: mockRepositories.length,
      total: mockRepositories.length,
      totalPages: 1,
    },
  });
});

// GET /api/v1/repositories/:id - Get specific repository
router.get('/:id', (req, res) => {
  const { id } = req.params;
  const repository = mockRepositories.find((r) => r.id === id);

  if (!repository) {
    return res.status(404).json({
      type: 'not_found',
      title: 'Repository Not Found',
      status: 404,
      detail: `Repository with id ${id} not found`,
    });
  }

  res.json(repository);
});

export default router;
