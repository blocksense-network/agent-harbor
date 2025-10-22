/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { useParams } from '@solidjs/router';
import { Title, Meta } from '@solidjs/meta';
import { TaskDetailsPage } from '../../components/task-details/TaskDetailsPage';

export default function TaskDetailsRoute() {
  const params = useParams();
  const taskId = params['id'];

  return (
    <>
      <Title>Agent Harbor — Task {taskId}</Title>
      <Meta name="description" content={`View details and monitor progress for task ${taskId}`} />
      <TaskDetailsPage taskId={taskId || ''} />
    </>
  );
}
