import { describe,it,expect } from 'vitest';
import { keyUsage,keyQuota,costLabel } from './keyUsage';
describe('actual key spend',()=>{
 it('uses actual usage instead of a resettable quota counter',()=>{expect(keyUsage({quota_used:99,usage:{total_actual_cost:0.0123,today_actual_cost:0}})).toEqual({total:0.0123,today:0});expect(costLabel(0.0123)).toBe('US$0.0123');});
 it('does not render an unavailable statistic as zero',()=>{expect(keyUsage({quota_used:0})).toEqual({total:null,today:null});expect(costLabel(null)).toBe('—');});
 it('keeps configured limits distinct and rejects malformed amounts',()=>{expect(keyQuota({quota:10,quota_used:4})).toEqual({limit:10,used:4,percentage:40});expect(keyQuota({quota:0,quota_used:3})).toBe(null);expect(keyUsage({usage:{total_actual_cost:'',today_actual_cost:-3}})).toEqual({total:null,today:null});});
});
