/**
 * 임시 Arrow 파일 정리 API
 * 
 * DB에 등록된 로그 폴더들에서 오래된 임시 Arrow 파일들을 삭제합니다.
 */

import { invoke } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { join } from '@tauri-apps/api/path';

/**
 * 임시 Arrow 파일 정리
 * 
 * @param maxAgeHours - 삭제할 파일의 최대 나이 (시간 단위, 기본값: 24시간)
 * @returns 삭제된 파일 수
 * 
 * @example
 * ```typescript
 * import { cleanupTempArrowFiles } from '$api/cleanup';
 * 
 * // 24시간 이상 된 임시 파일 삭제
 * const count = await cleanupTempArrowFiles(24);
 * console.log(`${count}개의 임시 파일 삭제됨`);
 * 
 * // 1시간 이상 된 임시 파일 삭제 (테스트용)
 * const count = await cleanupTempArrowFiles(1);
 * ```
 */
export async function cleanupTempArrowFiles(maxAgeHours: number = 24): Promise<number> {
    try {
        // AppData 디렉토리에서 test.db 경로 가져오기
        const appData = await appDataDir();
        const dbPath = await join(appData, 'test.db');
        
        console.log(`🧹 임시 파일 정리 시작 (DB: ${dbPath})`);
        
        const deletedCount = await invoke<number>('cleanup_temp_arrow_files', {
            dbPath,
            maxAgeHours
        });
        
        if (deletedCount > 0) {
            console.log(`✅ ${deletedCount}개의 임시 파일 삭제 완료`);
        } else {
            console.log('ℹ️  정리할 임시 파일 없음');
        }
        
        return deletedCount;
    } catch (error) {
        console.error('❌ 임시 파일 정리 실패:', error);
        throw error;
    }
}

/**
 * 애플리케이션 시작 시 자동으로 임시 파일 정리
 * 
 * 이 함수를 App 컴포넌트의 onMount에서 호출하세요.
 */
export async function autoCleanupOnStartup(): Promise<void> {
    try {
        console.log('🚀 애플리케이션 시작: 임시 파일 자동 정리 시작');
        await cleanupTempArrowFiles(24);
    } catch (error) {
        console.warn('⚠️  자동 정리 실패 (무시됨):', error);
    }
}
