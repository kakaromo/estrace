import type { EntryGenerator } from './$types';

export const entries: EntryGenerator = () => {
	// ID를 문자열로 변경 (숫자 -> 문자열)
	return [
		{ id: '1' },  // 문자열로 변환
		{ id: '2' },  // 추가 예제 ID
		{ id: '3' }   // 추가 예제 ID
	];
};

export const prerender = true;

// 필요한 경우 load 함수 추가
export const load = ({ params }) => {
	// params.id는 이미 문자열 타입입니다
	return {
		id: params.id
		// 추가 데이터 로딩 로직
	};
};