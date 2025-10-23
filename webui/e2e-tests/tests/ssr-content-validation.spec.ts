/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('SSR Content Validation', () => {
  test('SSR renders session cards with actual content', async ({ request }) => {
    const response = await request.get('http://localhost:3002/');
    expect(response.ok()).toBeTruthy();

    const html = await response.text();

    console.log('HTML length:', html.length);
    console.log('Contains <div id="app">:', html.includes('<div id="app">'));

    // Check for list structure
    console.log('Contains <ul role="list">:', html.includes('<ul role="list">'));
    console.log('Contains <li>:', html.includes('<li>'));

    // Check for session card content
    const hasSessionCardContent =
      html.includes('data-testid="task-card"') ||
      html.includes('session-card') ||
      html.includes('SessionCard');
    console.log('Has SessionCard markers:', hasSessionCardContent);

    // Check for specific content from mock data
    const hasPromptText =
      html.includes('Implement user authentication') ||
      html.includes('Add dark mode') ||
      html.includes('Fix memory leak') ||
      html.includes('Optimize database');
    console.log('Has prompt text from mock data:', hasPromptText);

    // Check for empty <li> tags
    const hasEmptyLi = html.includes('<li></li>') || html.match(/<li>\s*<\/li>/);
    console.log('Has empty <li> tags:', hasEmptyLi);

    // Extract a sample of <li> content
    const liMatch = html.match(/<li[^>]*>[\s\S]{0,200}/);
    if (liMatch) {
      console.log('Sample <li> content:', liMatch[0]);
    }

    // Count how many <li> tags exist
    const liCount = (html.match(/<li/g) || []).length;
    console.log('Total <li> tags:', liCount);

    // Verify SSR actually rendered session cards
    expect(hasPromptText || hasSessionCardContent).toBe(true);
    expect(hasEmptyLi).toBe(false);
  });
});
