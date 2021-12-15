import { splitForHighlight } from './components'

describe('findHighlightedParts', () => {
  test('should return empty in needle not found', ()=>{
    let parts = splitForHighlight("haystack", "needle");
    expect(parts).toStrictEqual([[false, "haystack"]])
  })
  
  test('case when single needle is found in the end', ()=>{
    let parts = splitForHighlight("One of the words", "words");
    expect(parts).toStrictEqual([[false, "One of the "], [true, "words"]])
  })

  test('case when single needle is found', ()=>{
    let parts = splitForHighlight("One of the words", "the");
    expect(parts).toStrictEqual([[false, "One of "], [true, "the"], [false, " words"]])
  })

  test('case when multiple needle are found', ()=>{
    let parts = splitForHighlight("One of the words in the text", "the");
    expect(parts).toStrictEqual([
      [false, "One of "],
      [true, "the"],
      [false, " words in "],
      [true, "the"],
      [false, " text"]
    ])
  })
})