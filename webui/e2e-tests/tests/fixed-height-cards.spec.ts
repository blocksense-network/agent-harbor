import { test, expect } from '@playwright/test';

test.describe('Fixed-Height Card Requirements', () => {
  test('active session cards always show exactly 3 activity rows', async ({ page }) => {
    await page.goto('/');
    
    // Find an active session card
    const activeCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/running|queued|provisioning/i') 
    }).first();
    
    await expect(activeCard).toBeVisible();
    
    // Count activity rows - should be exactly 3
    const activityRows = activeCard.locator('.space-y-0\\.5 > div');
    await expect(activityRows).toHaveCount(3);
    
    // Each row should have fixed height class h-4
    for (let i = 0; i < 3; i++) {
      const row = activityRows.nth(i);
      await expect(row).toHaveClass(/h-4/);
    }
  });

  test('active cards are pre-populated from SSR (no "Waiting for agent activity")', async ({ page }) => {
    await page.goto('/');
    
    // Check that no active cards show "Waiting for agent activity"
    const waitingText = page.locator('text=/waiting for agent activity/i');
    await expect(waitingText).not.toBeVisible();
    
    // Active cards should have visible activity content
    const activeCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/running/i') 
    }).first();
    
    if (await activeCard.isVisible()) {
      const activityRows = activeCard.locator('.space-y-0\\.5 > div');
      
      // At least one row should have visible text
      const hasContent = await activityRows.first().textContent();
      expect(hasContent?.trim().length).toBeGreaterThan(0);
    }
  });

  test('completed cards are compact (2 lines, no activity area)', async ({ page }) => {
    await page.goto('/');
    
    // Find a completed session card
    const completedCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/completed|failed|cancelled/i') 
    }).first();
    
    if (await completedCard.isVisible()) {
      // Should NOT have activity rows area
      const activityArea = completedCard.locator('.space-y-0\\.5');
      await expect(activityArea).not.toBeVisible();
      
      // Should only have title line and metadata line (2 lines total)
      const metadataLine = completedCard.locator('.flex.items-center.space-x-1\\.5');
      await expect(metadataLine).toBeVisible();
    }
  });

  test('merged cards are compact (2 lines, no activity area)', async ({ page }) => {
    await page.goto('/');
    
    // Find a merged session card
    const mergedCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/merged/i') 
    }).first();
    
    if (await mergedCard.isVisible()) {
      // Should NOT have activity rows area
      const activityArea = mergedCard.locator('.space-y-0\\.5');
      await expect(activityArea).not.toBeVisible();
    }
  });

  test('all metadata fits on single line', async ({ page }) => {
    await page.goto('/');
    
    const card = page.locator('[data-testid="task-card"]').first();
    await expect(card).toBeVisible();
    
    // Metadata line should exist and have flex row layout
    const metadataLine = card.locator('.flex.items-center.space-x-1\\.5').first();
    await expect(metadataLine).toBeVisible();
    
    // Should contain all metadata elements
    await expect(metadataLine.locator('text=📁')).toBeVisible();
    await expect(metadataLine.locator('text=🤖')).toBeVisible();
    await expect(metadataLine.locator('text=🕒')).toBeVisible();
  });

  test('activity rows maintain fixed height during updates', async ({ page }) => {
    await page.goto('/');
    
    const activeCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/running/i') 
    }).first();
    
    if (await activeCard.isVisible()) {
      const activityRows = activeCard.locator('.space-y-0\\.5 > div');
      
      // Measure initial heights
      const initialHeights = [];
      for (let i = 0; i < 3; i++) {
        const box = await activityRows.nth(i).boundingBox();
        initialHeights.push(box?.height || 0);
      }
      
      // Wait for potential SSE updates (2 seconds)
      await page.waitForTimeout(2000);
      
      // Measure heights again - should be identical
      for (let i = 0; i < 3; i++) {
        const box = await activityRows.nth(i).boundingBox();
        expect(box?.height).toBe(initialHeights[i]);
      }
    }
  });

  test('card height never changes (active sessions)', async ({ page }) => {
    await page.goto('/');
    
    const activeCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/running/i') 
    }).first();
    
    if (await activeCard.isVisible()) {
      // Measure initial card height
      const initialBox = await activeCard.boundingBox();
      const initialHeight = initialBox?.height || 0;
      
      // Wait for SSE updates
      await page.waitForTimeout(3000);
      
      // Measure height again - should be identical
      const finalBox = await activeCard.boundingBox();
      const finalHeight = finalBox?.height || 0;
      
      expect(finalHeight).toBe(initialHeight);
    }
  });

  test('empty activity rows render as transparent placeholders', async ({ page }) => {
    await page.goto('/');
    
    const activeCard = page.locator('[data-testid="task-card"]').filter({ 
      has: page.locator('text=/running/i') 
    }).first();
    
    if (await activeCard.isVisible()) {
      const activityRows = activeCard.locator('.space-y-0\\.5 > div');
      
      // Check if any row is empty (transparent)
      for (let i = 0; i < 3; i++) {
        const row = activityRows.nth(i);
        const hasTransparent = await row.evaluate((el) => 
          el.className.includes('text-transparent')
        );
        
        if (hasTransparent) {
          // Empty row should contain non-breaking space
          const text = await row.textContent();
          expect(text).toBe('\u00A0');
        }
      }
    }
  });
});
