import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TaskSkillRoutingCard } from './TaskSkillRoutingCard';

const taskSkillRouting = vi.fn();
vi.mock('../api', () => ({ api: { taskSkillRouting: (...args: unknown[]) => taskSkillRouting(...args) } }));

describe('TaskSkillRoutingCard', () => {
  beforeEach(() => { taskSkillRouting.mockReset(); });
  it('distinguishes recommendation, observed read and unverified compliance', async () => {
    taskSkillRouting.mockResolvedValue({
      taskId: 'task', turnId: 'turn', suggestions: [
        { skillId: 'blueprint-flow', title: 'Blueprint Flow', reason: '蓝图', score: 8 },
        { skillId: 'frontend-skill', title: 'Frontend', reason: '界面', score: 4 },
      ], readIds: ['blueprint-flow'], history: [
        { turnId: 'turn', eventType: 'read', skillId: 'blueprint-flow', atMs: 1 },
      ], adherence: 'notVerified',
    });
    render(<TaskSkillRoutingCard taskId="task" />);
    await waitFor(() => expect(screen.getByText('Blueprint Flow')).toBeInTheDocument());
    expect(screen.getByText('Frontend')).toBeInTheDocument();
    expect(screen.getByText('Skill read')).toBeInTheDocument();
    expect(screen.getByText('Not read')).toBeInTheDocument();
    expect(screen.getByText(/Skill compliance is not automatically verified/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /refresh skill records/i }));
    await waitFor(() => expect(taskSkillRouting).toHaveBeenCalledTimes(2));
  });
  it('shows no recommendation instead of inventing skill activity', async () => {
    taskSkillRouting.mockResolvedValue({
      taskId: 'task', turnId: null, suggestions: [], readIds: [], history: [], adherence: 'notVerified',
    });
    render(<TaskSkillRoutingCard taskId="task" />);
    await waitFor(() => expect(screen.getByText('No skill recommendation for the latest turn.')).toBeInTheDocument());
    expect(screen.queryByText('Skill read')).not.toBeInTheDocument();
  });
});
